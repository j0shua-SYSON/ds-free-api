//! Tool parsing -- validate tools/tool_choice and generate prompt injection text
//!
//! Since ds_core does not support native function calling, this module downgrades tool definitions
//! to natural-language descriptions and appends them to the prompt to guide model output.

use crate::openai_adapter::response::{TOOL_CALL_END, TOOL_CALL_START};
use crate::openai_adapter::types::{
    AllowedTools, AllowedToolsChoice, ChatCompletionsRequest, CustomTool, CustomToolFormat,
    FunctionDefinition, Tool, ToolChoice,
};

/// Extracted tool context
pub(crate) struct ToolContext {
    /// Format template + rules + examples (placed before the tool definitions)
    pub format_block: Option<String>,
    /// Formatted tool definition text
    pub defs_text: Option<String>,
    /// Behavior instructions appended based on tool_choice / parallel_tool_calls
    pub instruction_text: Option<String>,
}

fn has_tools(req: &ChatCompletionsRequest) -> bool {
    req.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false)
}

/// Extract and validate tool information from the request
///
/// Returns an empty ToolContext when tool_choice is none, generating no injection text.
pub(crate) fn extract(req: &ChatCompletionsRequest) -> Result<ToolContext, String> {
    let default_choice = if has_tools(req) {
        ToolChoice::Mode("auto".to_string())
    } else {
        ToolChoice::Mode("none".to_string())
    };
    let tool_choice = req.tool_choice.as_ref().unwrap_or(&default_choice);

    validate_tool_choice(tool_choice, req.tools.as_deref())?;

    if matches!(tool_choice, ToolChoice::Mode(m) if m == "none") {
        return Ok(ToolContext {
            format_block: None,
            defs_text: None,
            instruction_text: None,
        });
    }

    let mut instruction_lines = Vec::new();

    match tool_choice {
        ToolChoice::Mode(mode) => {
            if mode == "required" {
                instruction_lines.push("**Note: you must call one or more tools.**".to_string());
            }
        }
        ToolChoice::AllowedTools(AllowedToolsChoice { allowed_tools, .. }) => {
            build_allowed_tools_instruction(allowed_tools, &mut instruction_lines);
        }
        ToolChoice::Named(named) => {
            instruction_lines.push(format!(
                "**Note: you must call the '{}' tool.**",
                named.function.name
            ));
        }
        ToolChoice::Custom(custom) => {
            instruction_lines.push(format!(
                "**Note: you must call the '{}' custom tool.**",
                custom.custom.name
            ));
        }
    }

    if req.parallel_tool_calls == Some(false) {
        instruction_lines.push("**Note: only one tool may be called at a time.**".to_string());
    }

    let format_block = has_tools(req).then(|| build_tool_instruction_block(req));

    let defs_text = if has_tools(req) {
        let mut lines = vec!["You can use the following tools:".to_string()];
        for (i, tool) in req.tools.as_ref().unwrap().iter().enumerate() {
            lines.push(format_tool(tool, i)?);
        }
        Some(lines.join("\n"))
    } else {
        None
    };

    let instruction_text = if instruction_lines.is_empty() {
        None
    } else {
        Some(instruction_lines.join("\n"))
    };

    Ok(ToolContext {
        format_block,
        defs_text,
        instruction_text,
    })
}

fn validate_tool_choice(tc: &ToolChoice, tools: Option<&[Tool]>) -> Result<(), String> {
    match tc {
        ToolChoice::Mode(mode) => {
            if !matches!(mode.as_str(), "none" | "auto" | "required") {
                return Err(format!("invalid tool_choice mode: {}", mode));
            }
            if matches!(mode.as_str(), "auto" | "required")
                && tools.map(|t| t.is_empty()).unwrap_or(true)
            {
                return Err("tools must be provided when tool_choice is 'auto' or 'required'".into());
            }
            Ok(())
        }
        ToolChoice::Named(_) | ToolChoice::Custom(_) => {
            if tools.is_none() {
                return Err("tools must be provided when tool_choice specifies a named tool".into());
            }
            Ok(())
        }
        ToolChoice::AllowedTools(AllowedToolsChoice { allowed_tools, .. }) => {
            if tools.is_none() {
                return Err("tools must be provided when tool_choice specifies allowed_tools".into());
            }
            if !matches!(allowed_tools.mode.as_str(), "auto" | "required") {
                return Err(format!(
                    "allowed_tools.mode must be 'auto' or 'required', got: {}",
                    allowed_tools.mode
                ));
            }
            Ok(())
        }
    }
}

fn build_allowed_tools_instruction(allowed_tools: &AllowedTools, lines: &mut Vec<String>) {
    if let Some(tool_list) = &allowed_tools.tools {
        let names: Vec<String> = tool_list
            .iter()
            .filter_map(|v| v.get("function").and_then(|f| f.get("name")))
            .filter_map(|n| n.as_str().map(|s| s.to_string()))
            .collect();
        if !names.is_empty() {
            lines.push(format!(
                "**Note:** you may only choose from the following allowed tools: {}.",
                names.join(", ")
            ));
        }
    }

    if allowed_tools.mode == "required" {
        lines.push("**Note: you must call one or more tools.**".to_string());
    }
}

fn format_tool(tool: &Tool, idx: usize) -> Result<String, String> {
    match tool.ty.as_str() {
        "function" => {
            let func = tool.function.as_ref().ok_or_else(|| {
                format!("tools[{}] type 'function' requires a function definition", idx)
            })?;
            format_function(func)
        }
        "custom" => {
            let custom = tool
                .custom
                .as_ref()
                .ok_or_else(|| format!("tools[{}] type 'custom' requires a custom definition", idx))?;
            Ok(format_custom(custom))
        }
        _ => Err(format!("tools[{}] unsupported type: {}", idx, tool.ty)),
    }
}

fn format_function(func: &FunctionDefinition) -> Result<String, String> {
    if func.name.trim().is_empty() {
        return Err("function in tools is missing required field 'name'".into());
    }
    let params = serde_json::to_string(&func.parameters).unwrap_or_else(|_| "{}".into());
    let call_example = format!(
        "{TOOL_CALL_START}[{{\"name\": \"{}\", \"arguments\": {}}}]{TOOL_CALL_END}",
        func.name, params
    );
    let desc = func.description.as_deref().unwrap_or("").trim();
    let desc_block = if desc.is_empty() {
        "  no description".to_string()
    } else {
        format!("~~~markdown\n  {}\n~~~\n", desc)
    };
    Ok(format!(
        "- **{}** (function):\n  - call method: `{}`\n  - brief description:\n{}",
        func.name, call_example, desc_block,
    ))
}

/// Build the tool call instruction block: template -> rules -> dynamic correct examples
fn build_tool_instruction_block(req: &ChatCompletionsRequest) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Template
    lines.push("**Tool call format -- follow strictly:**".into());
    lines.push(String::new());
    lines.push("Wrap the JSON array in tool call markers:".into());
    lines.push(String::new());
    lines.push(format!(
        "{TOOL_CALL_START}[{{\"name\": \"tool_name\", \"arguments\": {{param_json}}}}]{TOOL_CALL_END}"
    ));
    lines.push(String::new());

    // Rules
    lines.push("**Rules:**".into());
    lines.push(String::new());
    lines.push(
        "**Core rule: when you decide to call a tool, your response may only contain the tool call text itself -- no explanations, prefixes, summaries, greetings, or any other extra content.**".into(),
    );
    lines.push(String::new());
    lines.push(format!("1. The JSON array must start with `{TOOL_CALL_START}` and end with `{TOOL_CALL_END}`, **fully wrapping** the array inside the markers."));
    lines.push("2. All tool calls must be placed in **one** JSON array, with multiple calls separated by commas.".into());
    lines.push(format!(
        "3. **Stop immediately** after outputting `{TOOL_CALL_END}` -- do not add any subsequent text, XML tags, or explanatory content."
    ));
    lines.push("4. Do not wrap tool calls inside a markdown code block.".into());
    lines.push("5. String parameter values must be wrapped in **double quotes** (JSON standard).".into());
    lines.push(format!(
        "6. When deciding to call a tool, the **first non-whitespace character** of the output must be `{TOOL_CALL_START}`."
    ));
    lines.push(format!(
        "7. The entire response may contain **only one `{TOOL_CALL_START}` block** -- do not output multiple `{TOOL_CALL_START}` blocks."
    ));
    lines.push(format!(
        "8. **Repeat:** the entire response may contain only one `{TOOL_CALL_START}` block -- do not repeat it. If you have already output one `{TOOL_CALL_START}` block, absolutely do not output a second one."
    ));
    lines.push(format!(
        "9. **Repeat:** it is forbidden to output any text before `{TOOL_CALL_START}`, including but not limited to explanations, confirmations, summaries, and greetings."
    ));
    lines.push("10. Do not place your reply or tool calls inside thinking content.".to_string());
    lines.push(
        "11. **Repeat:** thinking content (inside `<think>` tags) is for internal reasoning only -- do not place your final reply or tool calls inside `<think>` tags.".to_string(),
    );
    lines.push(String::new());

    let tool_names: Vec<String> = req
        .tools
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|t| t.function.as_ref().map(|f| f.name.clone()))
        .collect();
    let a = tool_names.first().map(|s| s.as_str()).unwrap_or("tool_a");

    // Correct examples (using actual tool names, with realistic parameters)
    lines.push("**Correct examples:**".into());
    lines.push(String::new());

    // Example A: single tool
    lines.push("**Example A** -- calling one tool:".into());
    lines.push(format!(
        "{TOOL_CALL_START}[{{\"name\": \"{a}\", \"arguments\": {}}}]{TOOL_CALL_END}",
        example_args(a)
    ));
    lines.push(String::new());

    // Example B: two tools in parallel
    if tool_names.len() >= 2 {
        let items: Vec<String> = tool_names[..2]
            .iter()
            .map(|n| format!("{{\"name\": \"{n}\", \"arguments\": {}}}", example_args(n)))
            .collect();
        lines.push("**Example B** -- calling multiple tools at once (one array containing all calls):".into());
        lines.push(String::new());
        lines.push(format!(
            "{TOOL_CALL_START}[{}]{TOOL_CALL_END}",
            items.join(", ")
        ));
        lines.push(String::new());
    }

    // Example C: three tools in parallel
    if tool_names.len() >= 3 {
        let items: Vec<String> = tool_names[..3]
            .iter()
            .map(|n| format!("{{\"name\": \"{n}\", \"arguments\": {}}}", example_args(n)))
            .collect();
        lines.push("**Example C** -- calling three tools at once (all calls in one array):".into());
        lines.push(String::new());
        lines.push(format!(
            "{TOOL_CALL_START}[{}]{TOOL_CALL_END}",
            items.join(", ")
        ));
        lines.push(String::new());
    }

    // Example D: nested parameters (still standard JSON when values are arrays or objects)
    if !tool_names.is_empty() {
        let d_name = tool_names.first().map(|s| s.as_str()).unwrap_or("tool_a");
        lines.push("**Example D** -- parameter values that are nested objects/arrays (still standard JSON):".into());
        lines.push(String::new());
        lines.push(format!(
            "{TOOL_CALL_START}[{{\"name\": \"{d_name}\", \"arguments\": {}}}]{TOOL_CALL_END}",
            example_nested_args(d_name)
        ));
        lines.push(String::new());
    }

    lines.join("\n")
}

/// Returns example argument strings based on the tool name
fn example_args(name: &str) -> String {
    let args: &str = match name {
        "Read" | "read_file" => r#""file_path": "/path/to/file""#,
        "Bash" | "execute_command" | "exec_command" => r#""command": "ls -la""#,
        "Write" | "write_to_file" => r#""file_path": "/path/to/file", "content": "hello""#,
        "Edit" => r#""file_path": "/path/to/file", "old_string": "foo", "new_string": "bar""#,
        "Glob" => r#""pattern": "**/*.rs", "path": "."#,
        "search_files" => r#""query": "TODO", "path": "."#,
        "get_weather" => r#""city": "Beijing""#,
        "get_time" => r#""timezone": "Asia/Shanghai""#,
        "list_files" => r#""path": "."#,
        _ => r#""key": "value""#,
    };
    format!("{{{args}}}")
}

/// Returns nested parameter examples (parameter values are arrays or objects)
fn example_nested_args(name: &str) -> String {
    match name {
        "Edit" => r#"{"file_path": "/path/to/file", "edits": [{"old_string": "foo", "new_string": "bar"}, {"old_string": "x", "new_string": "y"}]}"#.into(),
        _ => r#"{"config": {"enabled": true, "items": ["a", "b"]}}"#.into(),
    }
}

fn format_custom(custom: &CustomTool) -> String {
    let desc = custom.description.as_deref().unwrap_or("").trim();
    let method = match &custom.format {
        Some(CustomToolFormat::Text) => "text".into(),
        Some(CustomToolFormat::Grammar { grammar }) => {
            format!("grammar(syntax: {})", grammar.syntax)
        }
        None => "unconstrained".into(),
    };
    format!(
        "- **{}** (custom):\n  - call method: `{}`\n  - brief description: {}",
        custom.name,
        method,
        if desc.is_empty() { "no description" } else { desc },
    )
}
