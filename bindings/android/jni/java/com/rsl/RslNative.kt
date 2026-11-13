package com.rsl

/**
 * Kotlin 薄壳（T6.3）。生命周期：create → runStage ×N → free。
 * 所有规则逻辑在 Rust（librsl_core.so），Kotlin 侧改不到 —— 多端一致的前提。
 */
object RslNative {
    init {
        System.loadLibrary("rsl_core")
    }

    external fun nativeCreate(sourceJson: String): Long
    external fun nativeRunStage(
        handle: Long,
        stage: String,
        inputsJson: String,
        transportJson: String,
    ): String?

    external fun nativeFree(handle: Long)
    external fun lastError(): String
}

/** 传输注入：fixture 响应列表（L2）或 null（真实 HTTP）。 */
data class RslTransport(val responses: List<Map<String, Any>>? = null) {
    fun toJson(): String {
        if (responses == null) return "null"
        val arr = responses.joinToString(",") { r ->
            val body64 = android.util.Base64.encodeToString(
                (r["body"] as? String ?: "").toByteArray(),
                android.util.Base64.NO_WRAP,
            )
            """{"url":"${r["url"] ?: ""}","status":${r["status"] ?: 200},""" +
                """"content_type":"${r["content_type"] ?: "text/html; charset=utf-8"}","body_base64":"$body64"}"""
        }
        return """{"responses":[$arr]}"""
    }
}

/** 阶段输入（S§5.4 显式注入）。 */
data class RslInputs(
    val kw: String? = null,
    val page: Long? = null,
    val book: Map<String, Any>? = null,
    val chapter: Map<String, Any>? = null,
) {
    fun toJson(): String {
        fun m(k: String, v: Map<String, Any>?): String {
            if (v == null) return "\"$k\":null"
            val inner = v.entries.joinToString(",") { (k2, v2) ->
                val vs = when (v2) {
                    is String -> "\"$v2\""
                    is Number -> v2.toString()
                    is Boolean -> v2.toString()
                    else -> "\"$v2\""
                }
                "\"$k2\":$vs"
            }
            return "\"$k\":{$inner}"
        }
        return "{${m("book", book)},${m("chapter", chapter)},\"kw\":${kw?.let { "\"$it\"" } ?: "null"}," +
            "\"page\":${page ?: "null"}}"
    }
}

class RslSource(sourceJson: String) {
    private val handle: Long = RslNative.nativeCreate(sourceJson)

    init {
        require(handle != 0L) { "RSL 编译失败: ${RslNative.lastError()}" }
    }

    fun runStage(stage: String, inputs: RslInputs = RslInputs(), transport: RslTransport? = null): Map<String, Any?> {
        val json = RslNative.nativeRunStage(handle, stage, inputs.toJson(), transport?.toJson() ?: "null")
        require(json != null) {
            val parts = RslNative.lastError().split("|")
            "RSL ${parts.getOrElse(0) { "?" }} @${parts.getOrElse(1) { "" }}: ${parts.getOrElse(2) { "" }}"
        }
        // 轻量 JSON→Map（宿主可换 kotlinx.serialization / Gson）
        return parseFlatObject(json!!)
    }

    fun close() {
        RslNative.nativeFree(handle)
    }

    /** 极简顶层对象解析（值含字符串/数字/布尔/数组原始文本）。 */
    private fun parseFlatObject(json: String): Map<String, Any?> {
        val result = mutableMapOf<String, Any?>()
        // 正式实现应使用 org.json：JSONObject(json).let { ... }
        try {
            val obj = org.json.JSONObject(json)
            for (key in obj.keys()) {
                val v = obj.get(key)
                result[key] = if (v == org.json.JSONObject.NULL) null else v
            }
        } catch (_: Exception) {
            result["raw"] = json
        }
        return result
    }
}
