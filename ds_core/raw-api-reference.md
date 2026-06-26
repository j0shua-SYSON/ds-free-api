# DeepSeek Backend API Reference

This document describes the raw HTTP requests made by `DsClient` inside `ds_core` to the DeepSeek backend.

## General Information

### Base URLs

- `https://chat.deepseek.com/api/v0` — all API endpoints
- `https://fe-static.deepseek.com` — WASM file downloads

### Common Request Headers

| Header | Description |
|--------|------|
| `User-Agent` | Required; WAF bypass — the value must resemble a real browser UA |
| `Authorization: Bearer <token>` | Required for authenticated requests |
| `X-Ds-Pow-Response: <base64>` | Required for requests that require PoW |
| `X-Client-Version` | Client version number (currently `2.0.0`) |
| `X-Client-Platform` | Client platform |
| `X-Client-Locale` | Client locale |

### Response Envelope Format

All non-streaming responses use a unified `Envelope` wrapper:

```json
{
  "code": 0,
  "msg": "",
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": { ... }
  }
}
```

- `code != 0` -> system-level error (e.g., 40003 invalid token)
- `biz_code != 0` -> business-level error
- `biz_data` -> actual data

### PoW target_path Mapping

| Endpoint | target_path |
|------|-------------|
| completion | `/api/v0/chat/completion` |
| edit_message | `/api/v0/chat/edit_message` |
| upload_file | `/api/v0/file/upload_file` |

### Error Response Format

| Case | Format |
|------|------|
| Missing field | HTTP 422: `{"detail":[{"loc":"body.<field>"}]}` |
| Invalid token | HTTP 200: `{"code":40003,"msg":"Authorization Failed (invalid token)","data":null}` |
| Business error | HTTP 200: `{"code":0,"data":{"biz_code":<N>,"biz_msg":"<msg>","biz_data":null}}` |
| Login failure | HTTP 200: `{"code":0,"data":{"biz_code":2,"biz_msg":"PASSWORD_OR_USER_NAME_IS_WRONG"}}` |

---

## 0. Login

- **URL**: `POST /api/v0/users/login`
- **Headers**:
  - `User-Agent`: Required
  - `Content-Type: application/json`: Optional (not needed when the HTTP library sets it automatically)
- **Request body**:

```json
{
  "email": null,
  "mobile": "[phone_number]",
  "password": "<password>",
  "area_code": "+86",
  "device_id": "[any base64 value or empty string, but the field must not be omitted]",
  "os": "web"
}
```

- `email` / `mobile`: use one, pass `null` for the other
- `device_id`: required field (omitted -> 422), but the value can be empty or random
- `os`: required (omitted -> 422), must be `"web"`

- **Response**:

```json
{
  "code": 0,
  "msg": "",
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "code": 0,
      "msg": "",
      "user": {
        "id": "test",
        "token": "api-token",
        "email": "te****t@email.com",
        "mobile_number": "999******99",
        "area_code": "+86",
        "status": 0,
        "id_profile": { "provider": "WECHAT", "id": "test", "name": "test", "picture": "...", "locale": "zh_CN", "email": null },
        "id_profiles": [],
        "chat": { "is_muted": 0, "mute_until": null },
        "has_legacy_chat_history": false,
        "need_birthday": false
      }
    }
  }
}
```

- **Key field**: `data.biz_data.user.token` (the Bearer token for all subsequent requests)
- **Error**: `biz_code=2` / `biz_msg="PASSWORD_OR_USER_NAME_IS_WRONG"`

---

## 1. Create Session

- **URL**: `POST /api/v0/chat_session/create`
- **Headers**: `Authorization`, `User-Agent`
- **Request body**: `{}`
- **Response**:

```json
{
  "code": 0,
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "chat_session": {
        "id": "e6795fb3-272f-4782-87cf-6d6140b5bf76",
        "seq_id": 197895830,
        "agent": "chat",
        "model_type": "default",
        "title": null,
        "title_type": "WIP",
        "version": 0,
        "current_message_id": null,
        "pinned": false,
        "inserted_at": 1775732630.005,
        "updated_at": 1775732630.005
      },
      "ttl_seconds": 259200
    }
  }
}
```

- **Key field**: `data.biz_data.chat_session.id` (the `chat_session_id` used in subsequent completion requests)
- `ttl_seconds`: 259200 (3 days), session validity period

---

## 2. Fetch WASM

- **URL**: `GET https://fe-static.deepseek.com/chat/static/sha3_wasm_bg.<hash>.wasm`
- **Headers**: no authentication required, no User-Agent needed
- **Response**: approximately 26 KB, `Content-Type: application/wasm`, standard WASM format (`\x00asm` magic number)
- **Note**: the hash portion of the URL may change; it is recommended to make this configurable

---

## 3. Create PoW Challenge

- **URL**: `POST /api/v0/chat/create_pow_challenge`
- **Headers**: `Authorization`, `User-Agent`
- **Request body**:

```json
{
  "target_path": "/api/v0/chat/completion"
}
```

- **Response**:

```json
{
  "code": 0,
  "msg": "",
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "challenge": {
        "algorithm": "DeepSeekHashV1",
        "challenge": "7ffc9d19b6eed96a6fca68f8ffe30ee61035d4959e4180f187bf85b356016a96",
        "salt": "3bde54628ea8413fee87",
        "signature": "ce4678cf7a1290c2a7ac88c4195a5b8497e5fc4b0e8044e804f5a6f3af6fe462",
        "difficulty": 144000,
        "expire_at": 1775380966945,
        "expire_after": 300000,
        "target_path": "/api/v0/chat/completion"
      }
    }
  }
}
```

- Key fields: `challenge` (hash input prefix), `salt` (concatenation value), `difficulty` (target threshold), `expire_at` (expiration timestamp in ms)
- `algorithm`: always `"DeepSeekHashV1"`
- `expire_after`: 300000 ms = 5-minute validity period

---

## 4. Chat Completion

- **URL**: `POST /api/v0/chat/completion`
- **Headers**: `Authorization`, `User-Agent`, `X-Ds-Pow-Response` (must be recomputed for each request)
- **Request body**:

```json
{
  "chat_session_id": "<id from the create endpoint>",
  "parent_message_id": null,
  "model_type": "default",
  "prompt": "Hello",
  "ref_file_ids": ["file-xxx"],
  "thinking_enabled": true,
  "search_enabled": true,
  "preempt": false
}
```

- `model_type`: `"expert"` (default) | `"default"` | etc.
- `ref_file_ids`: array of file IDs returned after upload; remembered at the session level, so subsequent `edit_message` calls need not repeat them
- `preempt`: preemption mode (not currently used by the web client), default false
- **Response**: `text/event-stream` SSE stream

### SSE Event Format

**1. `ready` — session ready**

```
event: ready
data: {"request_message_id":1,"response_message_id":2,"model_type":"expert"}
```

A `ready` event is usually immediately followed by `event: update_session`; this is a normal session update and should not be mistaken for the end of the stream.

**2. `update_session` — session update**

```
event: update_session
data: {"updated_at":1775386361.526172}
```

**3. Incremental content — operator format**

All incremental updates use a unified data format combining `"p"` (path) and `"o"` (operator):

| Format | Example |
|------|------|
| `"p"` path + `"v"` value | `{"p":"response/status","v":"FINISHED"}` |
| `"p"` + `"o":"APPEND"` + `"v"` value | `{"p":"response/fragments/-1/content","o":"APPEND","v":","}` |
| `"p"` + `"o":"SET"` + `"v"` value | `{"p":"response/fragments/-1/elapsed_secs","o":"SET","v":0.95}` |
| `"p"` + `"o":"BATCH"` + `"v"` array | `{"p":"response","o":"BATCH","v":[{"p":"accumulated_token_usage","v":41},{"p":"quasi_status","v":"FINISHED"}]}` |
| bare `"v"` value | `{"v":"User"}` (continue appending to the previous `"p"` path) |
| full JSON object (initial snapshot) | `{"v":{"response":{"message_id":2,"fragments":[...]}}}` |

### Delta Parsing Algorithm

Complete delta parsing logic from the DeepSeek frontend source:

```javascript
class DeltaParser {
    constructor() {
        this.op = "SET";   // default operator
        this.path = "";    // default path
    }

    parse(event) {
        // path/op persists across events: subsequent events may omit p/o
        let op  = this.op  = event.o ?? this.op;
        let path = this.path = event.p ?? this.path;

        // non-BATCH: return a single operation
        if (op !== "BATCH")
            return [{ path, op, value: event.v }];

        // BATCH: decompose each item in the array
        let subParser = new DeltaParser;
        let results = [];
        for (let item of event.v) {
            let sub = subParser.parse(item);
            for (let s of sub)
                s.path = (path ? path + "/" : "") + s.path;
            results.push(...sub);
        }
        return results;
    }
}
```

**Key rules**:

| Rule | Description |
|------|------|
| `p` and `o` persist across events | subsequent events may omit `p`/`o`, inheriting the previous event's values |
| `o` defaults to `"SET"` | events without an `o` field use SET semantics |
| `APPEND` on strings = `+=` | pure incremental append |
| `BATCH` recursively decomposed | child `p` is prefixed with the parent path |
| only 3 operation types | `SET` (replace), `APPEND` (append), `BATCH` (batch) |

**State update engine logic**:

```javascript
switch (op) {
case "SET":
    target[resolvePath(lastPart)] = value;  // direct assignment
    break;
case "APPEND":
    if (typeof value === "string")
        target[resolvePath(lastPart)] += value;  // string concatenation
    else if (Array.isArray(value))
        // array merge (push or splice to a negative index position)
    break;
}
```

### SSE Stream State Paths

| Path/Field | Description |
|-----------|------|
| `response/fragments/-1/content` | content of the last fragment |
| `response/fragments/-1/elapsed_secs` | thinking/search elapsed time (seconds), THINK type only |
| `response/fragments/-1/status` | fragment status `WIP` -> `FINISHED` |
| `response/fragments/-{n}/status` | negative index marks any fragment as complete |
| `response/conversation_mode` | conversation mode: `"DEFAULT"` or `"DEEP_SEARCH"` |
| `response/has_pending_fragment` | true when a fragment is being processed in the background |
| `response/search_status` | `"SEARCHING"` -> `"FINISHED"` |
| `response/accumulated_token_usage` | cumulative token usage |
| `response/quasi_status` | end signal within BATCH: `"FINISHED"` or `"INCOMPLETE"` |
| `response/status` | primary status `WIP` -> `FINISHED` or `INCOMPLETE` |

### Fragment Structure

```typescript
{
  id: number,
  type: "THINK" | "RESPONSE"
      | "TOOL_SEARCH"            // search query (with queries + results)
      | "TOOL_OPEN"              // open link (with result + reference)
      | "TIP",                   // tip bar (with style + hide_on_wip)
  content: string | null,
  elapsed_secs?: number,         // THINK type: thinking elapsed time
  status?: "WIP" | "FINISHED",
  queries?: Array<{ query: string }>,
  results?: Array<{ url: string, title: string, snippet: string, ... }>,
  result?: { url: string, title: string, snippet: string, ... },
  reference?: { id: number, type: "TOOL_SEARCH" },
  style?: "WARNING",
  hide_on_wip?: boolean,
  references?: Array<{ id: number, type: "TOOL_SEARCH" | "TOOL_OPEN" }>,
  stage_id: number
}
```

### Thinking Content vs Actual Output

Differentiated by the `fragments[].type` field:

```
type == "THINK"     -> thinking content (appears only when thinking=ON)
type == "RESPONSE"  -> actual output content
```

### Stream Stage Order (thinking=ON, search=ON)

```
 1. SNAPSHOT    -> initial snapshot, fragments[0].type="THINK"
 2. THINKING    -> content APPEND accumulates thinking content
 3. THINK END   -> elapsed_secs SET
 4. TOOL_SEARCH -> APPEND TOOL_SEARCH fragment
 5. SEARCH      -> results SET (large number of results)
 6. SEARCH END  -> status="FINISHED"
 7. THINK(2)    -> APPEND new THINK fragment (evaluating search results)
 8. TOOL_OPEN   -> APPEND multiple TOOL_OPEN fragments
 9. OPEN END    -> status="FINISHED" (bulk mark)
10. THINK(3)    -> APPEND new THINK fragment (consolidating information)
11. RESPONSE    -> APPEND RESPONSE fragment
12. CONTENT     -> content APPEND accumulates output
13. REFERENCE   -> BATCH injects reference markers [reference:N]
14. TIP         -> APPEND TIP fragment
15. BATCH       -> accumulated_token_usage + quasi_status="FINISHED"
16. DONE        -> status="FINISHED"
```

### Stream Stage Order (thinking=OFF, search=OFF)

```
1. SNAPSHOT    -> initial snapshot, fragments[0].type="RESPONSE"
2. CONTENT     -> content APPEND
3. BATCH       -> accumulated_token_usage + quasi_status="FINISHED"
4. DONE        -> status="FINISHED"
```

### `hint` — Server-Side Hint/Error

```
event: hint
data: {"type":"error","content":"Content is too long. Please shorten it and try again.","clear_response":true,"finish_reason":"input_exceeds_limit"}
```

- `type`: `"error"` indicates an error hint; other values can be ignored
- `finish_reason`: `"input_exceeds_limit"` (input too long), `"rate_limit_reached"` (rate limited), etc.
- The hint event typically appears shortly after `ready`; the stream processor should proactively terminate upon receiving a hint.

### Stream End Sequence

**Normal completion**:
```
data: {"p":"response","o":"BATCH","v":[{"p":"accumulated_token_usage","v":139},{"p":"quasi_status","v":"FINISHED"}]}
data: {"p":"response/status","o":"SET","v":"FINISHED"}

event: update_session
data: {"updated_at":1778639258.866693}

event: title
data: {"content":"Rust Ownership Concepts Explained"}

event: close
data: {"click_behavior":"none","auto_resume":false}
```

**Manual interrupt**:
```
data: {"p":"response","o":"BATCH","v":[{"p":"accumulated_token_usage","v":39},{"p":"quasi_status","v":"INCOMPLETE"}]}
data: {"p":"response/status","v":"INCOMPLETE"}
```

**The most reliable end signal is `response/status` changing to `FINISHED` or `INCOMPLETE`.**

---

## 5. Edit Message

- **URL**: `POST /api/v0/chat/edit_message`
- **Headers**: `Authorization`, `User-Agent`, `X-Ds-Pow-Response`
- **Request body**:

```json
{
  "chat_session_id": "<session_id>",
  "message_id": 1,
  "prompt": "test again",
  "search_enabled": true,
  "thinking_enabled": true
}
```

- **Note**: `model_type` and `ref_file_ids` are not in the payload — both are passed in the first completion and remembered at the session level; subsequent edit_message calls inherit them.
- `message_id`: must already exist (passing `message_id=1` for an empty session returns `biz_code=26, "invalid message id"`)
- Editing generates a new `message_id`; obtain `response_message_id` from the SSE `ready` event for use in subsequent `stop_stream` calls.
- **Response**: same as `completion` (SSE stream)

---

## 6. Stop Stream

- **URL**: `POST /api/v0/chat/stop_stream`
- **Headers**: `Authorization`, `User-Agent`
- **Request body**:

```json
{
  "chat_session_id": "57bf7fb1-5fde-4d21-a08e-5dfa017216d5",
  "message_id": 2
}
```

- `chat_session_id`: the session ID from the create endpoint
- `message_id`: the response message ID to cancel. An edit request with `message_id=1` corresponds to response `message_id=2`.
- **PoW header is not required**
- **Purpose**: cancels the ongoing streaming output. Calling this endpoint after the client disconnects tells the DeepSeek side to stop further generation.

**Response**:
```json
{"code":0,"msg":"","data":{"biz_code":0,"biz_msg":"","biz_data":null}}
```

---

## 7. Delete Session

- **URL**: `POST /api/v0/chat_session/delete`
- **Headers**: `Authorization`, `User-Agent`
- **Request body**: `{"chat_session_id": "<session_id>"}`
- **Response**:

```json
{"code":0,"msg":"","data":{"biz_code":0,"biz_msg":"","biz_data":null}}
```

---

## 8. Update Title

- **URL**: `POST /api/v0/chat_session/update_title`
- **Headers**: `Authorization`, `User-Agent`
- **Request body**:

```json
{
  "chat_session_id": "<session_id>",
  "title": "test"
}
```

- **Response**:

```json
{
  "code": 0,
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "chat_session_updated_at": 1775382827.122839,
      "title": "test"
    }
  }
}
```

- **Error codes**: `biz_code=5` -> `EMPTY_CHAT_SESSION` (empty session cannot have a title set); `biz_code=1` -> `ILLEGAL_CHAT_SESSION_ID`

---

## 9. Upload File

- **URL**: `POST /api/v0/file/upload_file`
- **Headers**: `Authorization`, `User-Agent`, `X-Ds-Pow-Response` (target_path is `/api/v0/file/upload_file`)
- **Request body**: `multipart/form-data`, field name `file`

```
Content-Disposition: form-data; name="file"; filename="test.txt"
Content-Type: text/plain
```

- **Response**:

```json
{
  "code": 0,
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "id": "file-4387ddbe-efed-4459-83b0-ebb89db61f0f",
      "status": "PENDING",
      "file_name": "test.txt",
      "from_share": false,
      "file_size": 1000,
      "model_kind": "NORMAL",
      "token_usage": null,
      "error_code": null,
      "inserted_at": 1778644590.853,
      "updated_at": 1778644590.853,
      "is_image": false,
      "audit_result": null
    }
  }
}
```

- Key field: `data.biz_data.id` (used in subsequent completion's `ref_file_ids`)
- After upload, `status` is `PENDING`; poll `fetch_files` until `status=SUCCESS`.
- Status flow: `PENDING` -> `PARSING` -> `SUCCESS` (or `FAILED`)

---

## 10. Fetch File Status

- **URL**: `GET /api/v0/file/fetch_files?file_ids=<id>`
- **Headers**: `Authorization`, `User-Agent`
- **Response**:

```json
{
  "code": 0,
  "data": {
    "biz_code": 0,
    "biz_msg": "",
    "biz_data": {
      "files": [
        {
          "id": "file-xxx",
          "status": "SUCCESS",
          "file_name": "main.js",
          "from_share": false,
          "file_size": 2836902,
          "model_kind": "NORMAL",
          "token_usage": 619907,
          "error_code": null,
          "inserted_at": 1778644547.106,
          "updated_at": 1778644547.106,
          "is_image": false,
          "audit_result": null
        }
      ]
    }
  }
}
```

- Key field: `files[].status` -> `SUCCESS` indicates the upload is complete
- Status flow: `PENDING` -> `PARSING` -> `SUCCESS`
- `model_kind`: `"NORMAL"` (text/PDF) or `"VISION"` (image)
- `token_usage`: number of tokens consumed by file parsing (only available after SUCCESS)

---

## WASM Failure Handling

If DeepSeek updates the WASM file and PoW calculation fails:

1. `PowSolver` uses dynamic export probing (without hard-coding `__wbindgen_export_0`), automatically adapting to most WASM changes
2. If it still fails, update `wasm_url` in the config to point to the new WASM file URL
3. See the dynamic probing logic in `ds_core/src/accounts/pow.rs`

## WAF Bypass

- US IP addresses are blocked by the DeepSeek CloudFront WAF (HTTP 202 / x-amzn-waf-action)
- Configure a non-US proxy to bypass: `[proxy] url = "http://127.0.0.1:7890"`
- `wreq` uses BoringSSL to automatically emulate the Chrome 136 TLS fingerprint
