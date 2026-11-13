/* RSL C ABI（T6.1，由 crates/rsl-ffi 导出；此头为宿主侧声明）
 * 字符串一律 UTF-8；返回 JSON 文本；NULL = 失败（rsl_last_error 取错误）。
 */
#ifndef RSL_H
#define RSL_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RslEngine RslEngine;

/* 创建引擎（编译一份源）。
 * transport_json: {"responses":[{url,status,content_type,body_base64}]} = fixture 传输；NULL = 真实 HTTP。 */
RslEngine *rsl_engine_new(const char *source_json, const char *transport_json);

/* 执行阶段。inputs_json: {"kw","page","book":{…},"chapter":{…}}。返回 JSON 结果（调用方释放）。 */
char *rsl_engine_run_stage(RslEngine *engine, const char *stage, const char *inputs_json);

void rsl_engine_free(RslEngine *engine);

/* 编译校验：返回 {"ok":bool,"diagnostics":[…]}（调用方释放）。 */
char *rsl_compile(const char *source_json);

/* 最后一次错误："code|path|message"（调用方释放）。 */
char *rsl_last_error(void);

/* 释放 rsl_* 返回的字符串。 */
void rsl_string_free(char *p);

char *rsl_version(void);

#ifdef __cplusplus
}
#endif
#endif /* RSL_H */
