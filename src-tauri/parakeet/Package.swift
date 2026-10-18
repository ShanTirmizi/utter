// swift-tools-version: 6.2
//
// The Parakeet speech engine: a small command-line helper the app starts and
// talks to over stdin and stdout. It is a separate program, rather than a
// library linked into the app, so that the app itself keeps running on macOS 12
// while FluidAudio needs 14, and so that Swift and Core ML stay out of the
// Rust build. See docs/parakeet.md.
//
//   swift build -c release --package-path src-tauri/parakeet

import PackageDescription

let package = Package(
    name: "utter-parakeet",
    platforms: [.macOS(.v14)],
    dependencies: [
        // ASR only: no text-to-speech, diarization or text-normalization
        // engine, which would otherwise be compiled in and linked.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7", traits: [])
    ],
    targets: [
        .executableTarget(
            name: "utter-parakeet",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")],
            path: "Sources"
        )
    ]
)
