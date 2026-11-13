/* tslint:disable */
/* eslint-disable */

/**
 * WASM 引擎：fixture 传输（宿主 fetch 端口由 JS 侧实现后经 responses 注入）。
 */
export class RslEngine {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * 创建引擎。transport_json: {"responses":[{url,status,content_type,body_base64}]} 或 "null"。
     */
    constructor(source_json: string, transport_json: string);
    /**
     * 执行阶段。inputs_json: {"kw","page","book","chapter"}。
     * 返回 {"ok":true,"out":…} 或 {"ok":false,"error":{code,path,message}}。
     */
    run_stage(stage: string, inputs_json: string): string;
}

/**
 * 编译校验：输入源 JSON 文本，返回诊断 JSON。
 */
export function compile(source_json: string): string;

export function version(): string;
