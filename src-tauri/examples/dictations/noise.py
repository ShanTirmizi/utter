"""Mix steady noise into a 16-bit mono WAV, 15 dB below the speech level."""
import math, random, struct, sys, wave

source, target = sys.argv[1], sys.argv[2]
with wave.open(source) as w:
    params = w.getparams()
    samples = struct.unpack(f"<{w.getnframes()}h", w.readframes(w.getnframes()))
speech = [s for s in samples if abs(s) > 300] or [1]
rms = math.sqrt(sum(s * s for s in speech) / len(speech))
level = rms / (10 ** (15 / 20))
random.seed(source)
noisy, low = [], 0.0
for s in samples:
    # Brown-ish noise: white noise through a gentle low-pass, like a fan or room hum.
    low = 0.9 * low + 0.1 * random.gauss(0, 1)
    n = level * (0.5 * random.gauss(0, 1) + 2.2 * low)
    noisy.append(max(-32768, min(32767, int(s + n))))
with wave.open(target, "wb") as w:
    w.setparams(params)
    w.writeframes(struct.pack(f"<{len(noisy)}h", *noisy))
