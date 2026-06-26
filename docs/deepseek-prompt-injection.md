The following is a summary of `added_tokens` from [DeepSeek API Docs](https://api-docs.deepseek.com/zh-cn/quick_start/token_usage):

| Token                                                  | special  | normalized | Purpose                                                         |
| ------------------------------------------------------ | -------- | ---------- | --------------------------------------------------------------- |
| `<think></think>`                                      | false    | **true**   | **Reasoning chain container** (Chain-of-Thought). Reasoning models such as DeepSeek-R1 output their internal thinking process inside this tag before generating the final answer; typically collapsed in the UI. |
| `<｜fim▁hole｜>` / `<｜fim▁begin｜>` / `<｜fim▁end｜>` | false    | **true**   | **Fill-In-the-Middle (code infill)**. `begin` and `end` mark the prefix/suffix code blocks; `hole` marks the middle position the model must fill. |
| `<｜User｜>` / `<｜Assistant｜>`                       | false    | **true**   | **Role anchors**. Replace the traditional `User:` / `Assistant:` text prefixes as more robust structural delimiters, guarding against role-confusion attacks (prompt injection). |
| `<\|EOT\|>`                                              | **true** | **true**   | **End of Turn**. Marks the end of the current turn; one of the signals for the model to stop generating. |
| `<｜tool▁calls▁begin｜>` / `<｜tool▁calls▁end｜>`      | false    | **true**   | **Tool call list container**. Wraps all tools to be called in the current turn. |
| `<｜tool▁call▁begin｜>` / `<｜tool▁call▁end｜>`        | false    | **true**   | **Single tool call container**. Typically contains the function name and arguments in JSON format. |
| `<｜tool▁outputs▁begin｜>` / `<｜tool▁outputs▁end｜>`  | false    | **true**   | **Tool output list container**.                                  |
| `<｜tool▁output▁begin｜>` / `<｜tool▁output▁end｜>`    | false    | **true**   | **Single tool output container**.                                |
| `<｜tool▁sep｜>`                                       | false    | **true**   | **Tool separator**. Separates multiple tool calls or outputs within the same turn. |
| `<｜begin▁of▁sentence｜>` / `<｜end▁of▁sentence｜>`    | **true** | false      | **Sequence-level boundary markers** (BOS/EOS). Mark the physical start and end of the entire input/output sequence. |
| `<｜▁pad▁｜>`                                          | **true** | false      | **Padding token** (PAD). Used to align sequence lengths during batch inference; the model does not attend to it. |

Below are actual conversation tests on the DeepSeek web interface.

![DeepSeek web interface token test](assets/fig1.png)

From the figure above, after web backend filtering, the only usable tokens are `<think></think>`, `<｜User｜>`, and `<｜Assistant｜>`. Therefore:

- I opted to use `< | System | >` as a compromise for system prompt injection;
- ~~(Abandoned) Constraining the model to use a special pattern for tool calls and using `< | Tool | >` for tool call results;~~
- As shown in the figure below, an unclosed `<think>` tag can guide the model into a reasoning mode, enabling stronger rule injection (reminders);

![Unclosed think tag guiding reasoning](assets/fig2.png)

## Subsequent Experimental Findings

After real-world testing, using the native token `<｜tool▁calls▁begin｜>` as the primary tag caused severe model confusion. The backend likely applies special handling or filtering to the full-width `<｜...｜>` format.

A compromise was tried using `<|tool▁calls▁begin|>` / `<|tool▁calls▁end|>` as tool call tags:

- Replacing full-width `｜` with ASCII `|` avoids backend filtering while preserving the structural feel of native tokens
- **Surprisingly effective**: model recognition and adherence improved markedly, and hallucinations were greatly reduced
- Likely reason: the tokenizer already has token patterns for the `<|...|>` format, so the model has a stronger tendency to follow this "structural template"

**Current strategy: experiment-driven, incremental maintenance.**

- Primary tag: `<|tool▁calls▁begin|>` / `<|tool▁calls▁end|>`
- The fallback list is empty by default; append variants to `extra_starts` / `extra_ends` as hallucinated variants are discovered
- The `<|tool▁calls▁begin|>` format produces almost no hallucinations, avoiding a large amount of fallback-maintenance overhead
