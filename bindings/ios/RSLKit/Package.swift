// swift-tools-version:5.9
// RSLKit — Swift 薄壳（COMPILER §6：内存生命周期 / JSON↔结构体 / async 桥，约 400 行）
import PackageDescription

let package = Package(
    name: "RSLKit",
    products: [
        .library(name: "RSLKit", targets: ["RSLKit"]),
    ],
    targets: [
        .systemLibrary(name: "CRSL", path: "Sources/CRSL"),
        .target(
            name: "RSLKit",
            dependencies: ["CRSL"],
            path: "Sources/RSLKit",
            linkerSettings: [
                .linkedLibrary("rsl_core", .when(targetEnvironment: .ios)),
            ]
        ),
    ]
)
