// swift-tools-version: 6.0
import PackageDescription

// LibRaw search order: the copy Scripts/build_libraw.sh produces first (built against
// the app's real deployment target, universal, no external dependencies), then Homebrew
// on either prefix as a convenience for a plain `swift build`. Paths that do not exist
// are simply ignored.
let librawInclude = [
    "-IThirdParty/libraw/include",
    "-I/opt/homebrew/opt/libraw/include",
    "-I/usr/local/opt/libraw/include",
]
let librawLib = [
    "-LThirdParty/libraw/lib",
    "-L/opt/homebrew/opt/libraw/lib",
    "-L/usr/local/opt/libraw/lib",
]

let package = Package(
    name: "AwayPhotoRawEditor",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "AwayRawCore", targets: ["AwayRawCore"]),
        .executable(name: "AwayPhotoRawEditor", targets: ["AwayPhotoRawEditor"]),
        .executable(name: "awpr-cli", targets: ["awpr-cli"]),
    ],
    targets: [
        // C shim over LibRaw's C API. It also exposes the few fields (sizes.flip,
        // the visible-area dimensions) that the C API has no getter for — reading them
        // through the real struct instead of the byte-offset arithmetic the Windows
        // build had to resort to.
        .target(
            name: "CLibRawShim",
            cSettings: [.unsafeFlags(librawInclude)],
            linkerSettings: [.unsafeFlags(librawLib), .linkedLibrary("raw")]
        ),
        .target(
            name: "AwayRawCore",
            dependencies: ["CLibRawShim"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "AwayPhotoRawEditor",
            dependencies: ["AwayRawCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        .executableTarget(
            name: "awpr-cli",
            dependencies: ["AwayRawCore"],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
    ]
)
