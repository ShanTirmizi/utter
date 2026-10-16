// Parakeet TDT v3 speech-to-text on the Neural Engine, through FluidAudio,
// for Utter. The app starts this program once and keeps it running; each
// request is one line of JSON on stdin, and each reply one line on stdout.
//
//   {"load": "/path/to/parakeet-tdt-0.6b-v3"}
//       -> {"ok": true, "ms": 900}
//   {"transcribe": 160000, "terms": ["Anthropic", "Groq"]}
//   followed by that many 32-bit little-endian floats (16 kHz mono)
//       -> {"text": "...", "candidates": [...], "ms": 80}
//
// Each candidate is a place where the CTC model heard one of the terms more
// clearly than what Parakeet wrote: {"from": "grok", "to": "Groq", "start": 4,
// "end": 8}, by UTF-8 byte offsets into "text". The app decides which to use.
//
// Any request can instead get {"error": "..."}. The program exits when stdin
// closes, which is how the app stops it.
//
// The model folder holds the four Parakeet Core ML bundles and its vocabulary,
// and a `ctc` folder with the small CTC model used to spot the user's words.

import CoreML
import FluidAudio
import Foundation

// FluidAudio and Core ML log to stdout and stderr. Keep stdout for replies
// alone: write them to a copy of it, and send anything else to stderr.
let replies = FileHandle(fileDescriptor: dup(1), closeOnDealloc: true)
dup2(2, 1)

@MainActor func reply(_ object: [String: Any]) {
    var data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data("{\"error\":\"reply\"}".utf8)
    data.append(0x0A)
    replies.write(data)
}

func milliseconds(since start: Date) -> Int {
    Int(Date().timeIntervalSince(start) * 1000)
}

/// The CTC model and what was made from it for the current list of terms.
/// Building the rescorer reads the tokenizer, so it is kept until the terms
/// change.
struct Boosting: Sendable {
    let directory: URL
    let models: CtcModels
    let tokenizer: CtcTokenizer
    var terms: [String] = []
    var vocabulary: CustomVocabularyContext?
    var rescorer: VocabularyRescorer?

    var spotter: CtcKeywordSpotter {
        CtcKeywordSpotter(models: models, blankId: models.vocabulary.count)
    }

    mutating func prepare(_ wanted: [String]) async throws {
        if wanted == terms { return }
        terms = wanted
        let list = wanted.compactMap { text -> CustomVocabularyTerm? in
            let ids = tokenizer.encode(text)
            return ids.isEmpty ? nil : CustomVocabularyTerm(text: text, ctcTokenIds: ids)
        }
        if list.isEmpty {
            vocabulary = nil
            rescorer = nil
            return
        }
        let context = CustomVocabularyContext(terms: list)
        vocabulary = context
        rescorer = try await VocabularyRescorer.create(
            spotter: spotter, vocabulary: context, config: .default, ctcModelDirectory: directory)
    }

    /// Run the CTC model over the audio and look for the terms. Needs only
    /// the audio, so it runs alongside the transcription. Nil without terms.
    func spot(_ samples: [Float]) async throws -> CtcKeywordSpotter.SpotKeywordsResult? {
        guard let vocabulary else { return nil }
        return try await spotter.spotKeywordsWithLogProbs(
            audioSamples: samples, customVocabulary: vocabulary, minScore: nil)
    }

    /// Places where one of the user's terms may have been misheard: where
    /// the CTC model hears the term more clearly than what was written.
    func candidates(_ result: ASRResult, spot: CtcKeywordSpotter.SpotKeywordsResult?) -> [[String: Any]] {
        guard let spot, let vocabulary, let rescorer, let timings = result.tokenTimings, !timings.isEmpty,
            !spot.logProbs.isEmpty
        else {
            return []
        }
        let tuning = ContextBiasingConstants.rescorerConfig(forVocabSize: vocabulary.terms.count)
        let output = rescorer.ctcTokenEvaluateCandidates(
            transcript: result.text,
            tokenTimings: timings,
            logProbs: spot.logProbs,
            frameDuration: spot.frameDuration,
            cbw: tuning.cbw,
            marginSeconds: 0.5,
            minSimilarity: max(tuning.minSimilarity, vocabulary.minSimilarity)
        )
        // FluidAudio's own rescoring rebuilds the text from its word timings,
        // which drops the punctuation and capitals Parakeet wrote. So only
        // the replacements it would make are passed on, each with its place
        // in the text as written, for the app to apply.
        return output.candidates.compactMap { candidate in
            guard candidate.legacyOutcome == .applied, let range = candidate.baseTextUTF8Range else { return nil }
            return [
                "from": candidate.basePhrase, "to": candidate.canonicalTerm,
                "start": range.lowerBound, "end": range.upperBound,
            ]
        }
    }
}

var asr: AsrManager?
var boosting: Boosting?

@MainActor func load(_ path: String) async throws {
    let directory = URL(fileURLWithPath: path, isDirectory: true)
    let models = try AsrModels.loadLocal(from: directory, version: .v3)
    let manager = AsrManager(config: .default)
    try await manager.loadModels(models)
    asr = manager

    // The user's words are a bonus; transcription works without them.
    let ctc = directory.appendingPathComponent("ctc", isDirectory: true)
    do {
        boosting = Boosting(
            directory: ctc,
            models: try await CtcModels.loadDirect(from: ctc),
            tokenizer: try await CtcTokenizer.load(from: ctc))
    } catch {
        boosting = nil
        FileHandle.standardError.write(Data("[parakeet] word boosting unavailable: \(error)\n".utf8))
    }
}

@MainActor func readSamples(_ count: Int) -> [Float]? {
    var samples = [Float](repeating: 0, count: count)
    let read = samples.withUnsafeMutableBytes { fread($0.baseAddress, 4, count, stdin) }
    return read == count ? samples : nil
}

while let line = readLine() {
    guard let request = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any] else {
        reply(["error": "Unreadable request."])
        continue
    }
    let started = Date()
    if let path = request["load"] as? String {
        do {
            try await load(path)
            reply(["ok": true, "boosting": boosting != nil, "ms": milliseconds(since: started)])
        } catch {
            reply(["error": "Could not load Parakeet: \(error.localizedDescription)"])
        }
    } else if let count = request["transcribe"] as? Int {
        // Always read the audio, even to refuse it, so the stream stays in step.
        guard let samples = readSamples(count) else { exit(0) }
        guard let asr else {
            reply(["error": "No speech model is loaded."])
            continue
        }
        do {
            let terms = request["terms"] as? [String] ?? []
            if var current = boosting {
                try await current.prepare(terms)
                boosting = current
            }
            let current = boosting
            let spotting = Task { try await current?.spot(samples) }
            var state = TdtDecoderState.make(decoderLayers: await asr.decoderLayerCount)
            let result = try await asr.transcribe(samples, decoderState: &state)
            let spot = try await spotting.value
            let candidates = current?.candidates(result, spot: spot) ?? []
            reply(["text": result.text, "candidates": candidates, "ms": milliseconds(since: started)])
        } catch {
            reply(["error": "Transcription failed: \(error.localizedDescription)"])
        }
    } else {
        reply(["error": "Unknown request."])
    }
}
