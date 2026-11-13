// RSLKit — Swift 薄壳（COMPILER §6）。所有规则逻辑在 Rust，Swift 侧改不到。
import CRSL
import Foundation

public struct RSLDiagnostic: Decodable, Sendable {
    public let code: String
    public let severity: String
    public let path: String
    public let message: String
    public let hint: String
}

public struct RSLCompileResult: Decodable, Sendable {
    public let ok: Bool
    public let diagnostics: [RSLDiagnostic]
}

/// 阶段执行输入（S§5.4 显式注入：无隐式提升）。
public struct RSLInputs: Encodable, Sendable {
    public var kw: String?
    public var page: Int?
    public var book: [String: RSLValue]?
    public var chapter: [String: RSLValue]?

    public init(kw: String? = nil, page: Int? = nil, book: [String: RSLValue]? = nil, chapter: [String: RSLValue]? = nil) {
        self.kw = kw
        self.page = page
        self.book = book
        self.chapter = chapter
    }
}

/// JSON 值（宿主注入用；详细字段由 out 契约决定）。
public enum RSLValue: Codable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else { self = .null }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .string(let s): try c.encode(s)
        case .number(let n): try c.encode(n)
        case .bool(let b): try c.encode(b)
        case .null: try c.encodeNil()
        }
    }
}

public enum RSLError: Error, Sendable {
    case engineCreation(String)
    case stageError(code: String, path: String, message: String)
    case invalidUTF8
}

/// 引擎句柄（一个源一个实例；编译一次，S§12.1）。
public final class RSLEngine: @unchecked Sendable {
    private var handle: OpaquePointer?
    private let queue = DispatchQueue(label: "rsl.engine")

    /// 用源 JSON 创建引擎；fixtureResponses 非空时使用 fixture 传输（L2）。
    public init(sourceJSON: String, fixtureResponses: [[String: Any]] = []) throws {
        var transportJSON: String? = nil
        if !fixtureResponses.isEmpty {
            var items: [[String: Any]] = []
            for r in fixtureResponses {
                var item: [String: Any] = [:]
                item["url"] = r["url"] ?? ""
                item["status"] = r["status"] ?? 200
                item["content_type"] = r["content_type"] ?? "text/html; charset=utf-8"
                if let body = r["body"] as? String {
                    item["body_base64"] = Data(body.utf8).base64EncodedString()
                }
                items.append(item)
            }
            if let data = try? JSONSerialization.data(withJSONObject: ["responses": items]),
               let s = String(data: data, encoding: .utf8) {
                transportJSON = s
            }
        }
        let transportPtr = transportJSON.map { ($0 as NSString).utf8String }
        guard let h = rsl_engine_new(sourceJSON, transportPtr) else {
            throw RSLError.engineCreation(Self.takeLastError())
        }
        handle = OpaquePointer(h)
    }

    deinit {
        if let h = handle {
            rsl_engine_free(h)
        }
    }

    /// 执行阶段（async 桥）。
    public func run(stage: String, inputs: RSLInputs = .init()) async throws -> [String: Any] {
        try await withCheckedThrowingContinuation { cont in
            queue.async {
                do {
                    let result = try self.runSync(stage: stage, inputs: inputs)
                    cont.resume(returning: result)
                } catch {
                    cont.resume(throwing: error)
                }
            }
        }
    }

    /// 同步执行（内部用）。
    func runSync(stage: String, inputs: RSLInputs) throws -> [String: Any] {
        guard let h = handle else { throw RSLError.engineCreation("engine freed") }
        let inputsData = try JSONEncoder().encode(inputs)
        let inputsJSON = String(data: inputsData, encoding: .utf8) ?? "{}"
        guard let raw = rsl_engine_run_stage(h, stage, inputsJSON) else {
            let err = Self.takeLastError().split(separator: "|").map(String.init)
            throw RSLError.stageError(
                code: err.count > 0 ? err[0] : "unknown",
                path: err.count > 1 ? err[1] : "",
                message: err.count > 2 ? err[2] : ""
            )
        }
        defer { rsl_string_free(raw) }
        guard let data = Data(bytes: raw, count: strlen(raw)) as Data?,
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw RSLError.invalidUTF8
        }
        return obj
    }

    static func takeLastError() -> String {
        guard let raw = rsl_last_error() else { return "unknown" }
        defer { rsl_string_free(raw) }
        return String(cString: raw)
    }
}

/// 编译校验（不创建引擎；编辑器实时 lint 用）。
public func rslCompileCheck(sourceJSON: String) throws -> RSLCompileResult {
    guard let raw = rsl_compile(sourceJSON) else { throw RSLError.invalidUTF8 }
    defer { rsl_string_free(raw) }
    let data = Data(bytes: raw, count: strlen(raw))
    return try JSONDecoder().decode(RSLCompileResult.self, from: data)
}

public func rslVersion() -> String {
    guard let raw = rsl_version() else { return "?" }
    defer { rsl_string_free(raw) }
    return String(cString: raw)
}
