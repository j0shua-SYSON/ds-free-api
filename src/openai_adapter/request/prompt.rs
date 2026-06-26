//! Prompt building -- convert OpenAI messages to DeepSeek native tag format
//!
//! Uses `<｜System｜>`, `<｜User｜>`, `<｜Assistant｜>`, `<｜tool▁outputs▁begin｜>` as role markers.
//! If the request contains tool definitions or behavior instructions, they are embedded in the
//! unclosed `<think>` block after the last `<｜Assistant｜>`, ensuring the tool context is always
//! adjacent to the model's generation position.

use super::tools::ToolContext;
use crate::openai_adapter::response::{TOOL_CALL_END, TOOL_CALL_START};
use crate::openai_adapter::types::{ChatCompletionsRequest, ContentPart, Message, MessageContent};

/// Merge consecutive messages with the same role to prevent the DeepSeek model from being confused by consecutive same-role tags
fn merge_messages(messages: &[Message]) -> Vec<Message> {
    let mut merged: Vec<Message> = Vec::new();
    for msg in messages {
        if let Some(last) = merged.last_mut()
            && last.role == msg.role
            && msg.role != "tool"
        // tool messages are grouped and merged by build()
        {
            // merge content
            if let Some(ref content) = msg.content {
                match &mut last.content {
                    Some(last_content) => match (last_content, content) {
                        (MessageContent::Text(a), MessageContent::Text(b)) => {
                            a.push('\n');
                            a.push_str(b);
                        }
                        (MessageContent::Parts(a), MessageContent::Parts(b)) => {
                            a.extend(b.clone());
                        }
                        // different types -> convert both to text and concatenate
                        (last_c, new_c) => {
                            let new_text = format_content(new_c);
                            let last_text = format_content(last_c);
                            *last_c = MessageContent::Text(format!("{}\n{}", last_text, new_text));
                        }
                    },
                    None => {
                        last.content = msg.content.clone();
                    }
                }
            }
            // merge tool_calls
            if let Some(ref calls) = msg.tool_calls {
                match &mut last.tool_calls {
                    Some(last_calls) => last_calls.extend(calls.clone()),
                    None => last.tool_calls = msg.tool_calls.clone(),
                }
            }
            // overwrite fields: take the value from the last message
            if msg.name.is_some() {
                last.name.clone_from(&msg.name);
            }
            if msg.tool_call_id.is_some() {
                last.tool_call_id.clone_from(&msg.tool_call_id);
            }
            if msg.function_call.is_some() {
                last.function_call.clone_from(&msg.function_call);
            }
            if msg.refusal.is_some() {
                last.refusal.clone_from(&msg.refusal);
            }
            if msg.audio.is_some() {
                last.audio.clone_from(&msg.audio);
            }
            continue;
        }
        merged.push(msg.clone());
    }
    merged
}

/// Generate the prompt text corresponding to response_format
fn format_response_text(rf: &crate::openai_adapter::types::ResponseFormat) -> String {
    match rf.ty.as_str() {
        "json_object" => {
            "Output valid JSON objects directly, without any markdown code block markers or other explanatory text.".into()
        }
        "json_schema" => {
            let schema_text = rf
                .json_schema
                .as_ref()
                .map(|s| serde_json::to_string(s).unwrap_or_default())
                .unwrap_or_default();
            if schema_text.is_empty() {
                "Output in JSON format.".into()
            } else {
                format!(
                    "Output in JSON format, following the schema below:\n\n~~~json\n{}\n~~~",
                    schema_text
                )
            }
        }
        "text" => String::new(),
        _ => format!("Output in {} format.", rf.ty),
    }
}

/// Build the prompt string in DeepSeek native tag format
/// Order: [system (with reminder)] [historical user/tool/assistant turns...] <｜Assistant｜><think>[reminder]
pub(crate) fn build(req: &ChatCompletionsRequest, tool_ctx: &ToolContext) -> String {
    let messages = merge_messages(&req.messages);
    let mut parts: Vec<String> = Vec::with_capacity(messages.len());
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role == "tool" {
            let mut tool_contents = Vec::new();
            while i < messages.len() && messages[i].role == "tool" {
                if let Some(c) = &messages[i].content {
                    tool_contents.push(format_content(c));
                }
                i += 1;
            }
            let inner: String = tool_contents
                .iter()
                .map(|c| format!("<｜tool▁output▁begin｜>{}<｜tool▁output▁end｜>", c))
                .collect();
            parts.push(format!(
                "<｜tool▁outputs▁begin｜>{}<｜tool▁outputs▁end｜>",
                inner
            ));
        } else {
            parts.push(format_message(&messages[i]));
            i += 1;
        }
    }

    let mut tool_sections: Vec<String> = Vec::new();

    if let Some(text) = tool_ctx.format_block.as_deref() {
        tool_sections.push(format!("### Format specification\n{}", text));
    }
    if let Some(text) = tool_ctx.defs_text.as_deref() {
        tool_sections.push(format!("### Tool definitions\n{}", text));
    }
    if let Some(text) = tool_ctx.instruction_text.as_deref() {
        tool_sections.push(format!("### Call instructions\n{}", text));
    }

    let mut reminder_parts: Vec<String> = Vec::new();

    if !tool_sections.is_empty() {
        reminder_parts.push(format!("## Tool calls\n{}", tool_sections.join("\n\n")));
    }

    // response_format downgrade: inject format constraints into the <arg_key> block
    let format_text = req
        .response_format
        .as_ref()
        .map(format_response_text)
        .unwrap_or_default();
    if !format_text.is_empty() {
        reminder_parts.push(format!("## Output format\n{}", format_text));
    }

    if !reminder_parts.is_empty() {
        let reminder_body = reminder_parts.join("\n\n");

        // Inject the full reminder into the System section tail (without the "Hmm" prefix, with tool definitions)
        let sys_content = format!("\n\n{}", reminder_body);
        if let Some(sys) = parts.iter_mut().find(|p| p.starts_with("<｜System｜>")) {
            if let Some(end) = sys.rfind('\n') {
                sys.insert_str(end, &sys_content);
            }
        } else {
            parts.insert(0, format!("<｜System｜>{}\n", sys_content));
        }

        // The <think> block does not contain tool definitions, only format specifications and call instructions
        let mut think_sections: Vec<String> = Vec::new();
        if let Some(text) = tool_ctx.format_block.as_deref() {
            think_sections.push(format!("### Format specification\n{}", text));
        }
        if let Some(text) = tool_ctx.instruction_text.as_deref() {
            think_sections.push(format!("### Call instructions\n{}", text));
        }
        let mut think_parts: Vec<String> = Vec::new();
        if !think_sections.is_empty() {
            think_parts.push(format!("## Tool calls\n{}", think_sections.join("\n\n")));
        }
        // response_format only in think
        let think_format_text = req
            .response_format
            .as_ref()
            .map(format_response_text)
            .unwrap_or_default();
        if !think_format_text.is_empty() {
            think_parts.push(format!("## Output format\n{}", think_format_text));
        }
        if !think_parts.is_empty() {
            let think_reminder = format!(
                "Hmm, the system just reminded me to follow the instructions below:\n\n{}",
                think_parts.join("\n\n")
            );
            parts.push(format!("<｜Assistant｜><think>{}\n", think_reminder));
        }
    }

    // Ensure there is a trailing <｜Assistant｜> for split_history_prompt to use as a split point
    if !parts.iter().any(|p| p.starts_with("<｜Assistant｜>")) {
        parts.push("<｜Assistant｜>\n".to_string());
    }

    parts.join("")
}

fn role_tag(role: &str) -> String {
    let mut r = role.to_string();
    if let Some(c) = r.get_mut(0..1) {
        c.make_ascii_uppercase();
    }
    format!("<｜{}｜>", r)
}

fn format_message(msg: &Message) -> String {
    let body = match msg.role.as_str() {
        "assistant" => format_assistant(msg),
        "tool" => format_tool(msg),
        "function" => format_function(msg),
        _ => format_generic(msg),
    };
    let tag = if msg.role == "tool" {
        String::new() // tool messages use their own tags, <｜Tool｜> is not needed
    } else {
        role_tag(&msg.role)
    };
    let prefix = if msg.role == "user" {
        "<｜end▁of▁sentence｜>"
    } else {
        ""
    };
    format!("{}{}{}", prefix, tag, body)
}

fn format_generic(msg: &Message) -> String {
    let mut parts = Vec::new();
    if let Some(name) = &msg.name {
        parts.push(format!("(name: {name})"));
    }
    if let Some(content) = &msg.content {
        parts.push(format_content(content));
    }
    parts.join("\n")
}

fn format_assistant(msg: &Message) -> String {
    let mut parts = Vec::new();
    if let Some(content) = &msg.content {
        parts.push(format_content(content));
    }
    if let Some(tool_calls) = &msg.tool_calls {
        let items: Vec<String> = tool_calls
            .iter()
            .filter_map(|tc| {
                tc.function.as_ref().map(|func| {
                    let args = serde_json::from_str::<serde_json::Value>(&func.arguments)
                        .unwrap_or(serde_json::Value::Null);
                    format!(
                        "{{\"name\": {}, \"arguments\": {}}}",
                        serde_json::to_string(&func.name).unwrap_or_else(|_| "\"\"".into()),
                        serde_json::to_string(&args).unwrap_or_else(|_| "null".into()),
                    )
                })
            })
            .collect();
        parts.push(format!(
            "{TOOL_CALL_START}\n[{}]\n{TOOL_CALL_END}",
            items.join(", ")
        ));
    }
    if let Some(fc) = &msg.function_call {
        let args = serde_json::from_str::<serde_json::Value>(&fc.arguments)
            .unwrap_or(serde_json::Value::Null);
        let item = format!(
            "{{\"name\": {}, \"arguments\": {}}}",
            serde_json::to_string(&fc.name).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&args).unwrap_or_else(|_| "null".into()),
        );
        parts.push(format!("{TOOL_CALL_START}\n[{item}]\n{TOOL_CALL_END}"));
    }
    if let Some(refusal) = &msg.refusal {
        parts.push(format!("(refusal: {refusal})"));
    }
    parts.join("\n")
}

fn format_tool(msg: &Message) -> String {
    let content = msg.content.as_ref().map(format_content).unwrap_or_default();
    format!(
        "<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>{}<｜tool▁output▁end｜><｜tool▁outputs▁end｜>",
        content
    )
}

fn format_function(msg: &Message) -> String {
    let mut parts = Vec::new();
    if let Some(name) = &msg.name {
        parts.push(format!("(name: {name})"));
    }
    if let Some(content) = &msg.content {
        parts.push(format_content(content));
    }
    parts.join("\n")
}

fn format_content(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Parts(parts) => {
            parts.iter().map(format_part).collect::<Vec<_>>().join("\n")
        }
    }
}

fn format_part(part: &ContentPart) -> String {
    match part.ty.as_str() {
        "text" => part.text.clone().unwrap_or_default(),
        "refusal" => part.refusal.clone().unwrap_or_default(),
        "image_url" => part.image_url.as_ref().map_or_else(
            || "[image]".to_string(),
            |img| {
                if img.url.starts_with("http://") || img.url.starts_with("https://") {
                    format!("[please visit this link: {}]", img.url)
                } else {
                    let detail = img.detail.as_deref().unwrap_or("auto");
                    format!("[image: detail={detail}]")
                }
            },
        ),
        "input_audio" => {
            let fmt = part
                .input_audio
                .as_ref()
                .map(|a| a.format.as_str())
                .unwrap_or("unknown");
            format!("[audio: format={fmt}]")
        }
        "file" => {
            let filename = part
                .file
                .as_ref()
                .and_then(|f| f.filename.as_deref())
                .unwrap_or("unknown");
            let desc = part.text.as_deref().filter(|t| !t.is_empty());
            desc.map_or_else(
                || format!("[file: filename={filename}]"),
                |d| format!("[file: {d} (filename={filename})]"),
            )
        }
        _ => format!("[unsupported content type: {}]", part.ty),
    }
}
