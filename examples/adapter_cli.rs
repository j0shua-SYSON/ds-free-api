//! Unified protocol debug CLI
//!
//! Accepts an OpenAI JSON request body; supports raw DeepSeek SSE output, converted OpenAI SSE output, or a side-by-side comparison of both.
//!
//! Usage:
//!   Interactive mode: cargo run --example adapter_cli
//!   Script mode:      cargo run --example adapter_cli -- source examples/adapter_cli-script.txt
//!
//! Commands:
//!   chat <json_file>                       - Converted OpenAI output
//!   raw <json_file>                        - Raw DeepSeek SSE (before conversion)
//!   compare <json_file>                    - Side-by-side comparison of both streams
//!   concurrent <n> <json_file>             - Concurrent requests
//!   models                                 - List available models
//!   model <id>                             - Query a single model
//!   status                                 - View account pool status
//!   source <file>                          - Read and execute commands from a file
//!   quit | exit                            - Quit and clean up

use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use ds_free_api::{ChatCompletionsRequest, ChatOutput, Config, OpenAIAdapter, StreamResponse};
use futures::{StreamExt, future::join_all};
use std::io::{self, Read, Write};
use std::path::Path;

static DEMO_COUNTER: AtomicU64 = AtomicU64::new(0);

fn demo_req_id() -> String {
    format!("demo-{:x}", DEMO_COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn read_line_lossy() -> io::Result<String> {
    let mut buf = Vec::new();
    let mut handle = io::stdin().lock();
    loop {
        let mut byte = [0u8; 1];
        match handle.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                if byte[0] != b'\r' {
                    buf.push(byte[0]);
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info")).init();

    let (config, _config_path) = Config::load_with_args(std::env::args())?;
    println!("[Initializing...]");
    let adapter = OpenAIAdapter::new(&config).await?;
    println!(
        "[Ready] Commands: chat | raw | compare | concurrent | models | model | status | source | quit"
    );

    let mut stdout = io::stdout();

    loop {
        print!("> ");
        stdout.flush()?;

        let line = read_line_lossy()?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if handle_line(line, &adapter).await? {
            break;
        }
    }

    println!("[Shutting down...]");
    adapter.shutdown().await;
    println!("[Shutdown complete]");

    Ok(())
}

fn parse_args<'a>(parts: &'a [&'a str]) -> (Vec<&'a str>, bool) {
    let raw = parts.iter().any(|p| *p == "--raw" || *p == "-r");
    let positional: Vec<_> = parts
        .iter()
        .filter(|p| **p != "--raw" && **p != "-r")
        .copied()
        .collect();
    (positional, raw)
}

async fn handle_line(line: &str, adapter: &OpenAIAdapter) -> anyhow::Result<bool> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(false);
    }

    let cmd = parts[0];
    match cmd {
        "status" => {
            println!("[Account Status]");
            for (i, s) in adapter.account_statuses().iter().enumerate() {
                let email = if s.email.is_empty() { "-" } else { &s.email };
                let mobile = if s.mobile.is_empty() { "-" } else { &s.mobile };
                println!("  [{}] {} / {}", i + 1, email, mobile);
            }
        }

        "chat" if parts.len() >= 2 => {
            let file = parts[1];
            let body = load_json(file)?;
            let rid = demo_req_id();
            println!(">>> chat: {} [req={}]", file, rid);
            let req = serde_json::from_slice::<ChatCompletionsRequest>(&body)?;
            let result = adapter.chat_completions(req, &rid).await?;
            println!("[account: {}]", result.account_id);
            match result.data {
                ChatOutput::Stream(mut s) => {
                    use futures::StreamExt;
                    // ChunkStream → print each chunk as JSON line
                    while let Some(chunk) = s.next().await {
                        match chunk {
                            Ok(c) => println!("{}", serde_json::to_string(&c).unwrap()),
                            Err(e) => eprintln!("stream error: {}", e),
                        }
                    }
                }
                ChatOutput::Json(json) => println!("{}", serde_json::to_string(&json).unwrap()),
            }
        }

        "raw" if parts.len() >= 2 => {
            let file = parts[1];
            let body = load_json(file)?;
            let rid = demo_req_id();
            println!(">>> raw: {} [req={}]", file, rid);
            let mut result = adapter.raw_chat_completions_stream(&body, &rid).await?;
            println!("[account: {}]", result.account_id);
            print_stream(&mut result.data, true).await;
        }

        "compare" if parts.len() >= 2 => {
            let file = parts[1];
            let body = load_json(file)?;
            println!(">>> compare: {}", file);

            // Raw stream
            let rid1 = demo_req_id();
            println!(
                "\n═══ RAW DEEPSEEK SSE [req={}] ═════════════════════════════════",
                rid1
            );
            let raw_result = adapter.raw_chat_completions_stream(&body, &rid1).await?;
            println!("[account: {}]", raw_result.account_id);
            consume_stream(raw_result.data, |bytes| {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    println!("  {}", line);
                }
            })
            .await;

            // Converted stream
            let rid2 = demo_req_id();
            println!(
                "\n═══ CONVERTED OPENAI SSE [req={}] ═════════════════════════════",
                rid2
            );
            let conv_req = serde_json::from_slice::<ChatCompletionsRequest>(&body)?;
            let converted_result = adapter.chat_completions(conv_req, &rid2).await?;
            println!("[account: {}]", converted_result.account_id);
            match converted_result.data {
                ChatOutput::Stream(mut s) => {
                    use futures::StreamExt;
                    while let Some(chunk) = s.next().await {
                        match chunk {
                            Ok(c) => println!("  {}", serde_json::to_string(&c).unwrap()),
                            Err(e) => eprintln!("stream error: {}", e),
                        }
                    }
                }
                ChatOutput::Json(_) => {}
            }

            println!("\n═══ END ════════════════════════════════════════════════════");
        }

        "concurrent" if parts.len() >= 3 => {
            let (positional, raw) = parse_args(&parts);
            let count: usize = match positional[1].parse() {
                Ok(n) if n > 0 => n,
                _ => {
                    eprintln!("[Error] Concurrency count must be a positive integer");
                    return Ok(false);
                }
            };
            let file = positional[2];
            let body = load_json(file)?;
            println!(">>> concurrent: count={}, file={}", count, file);
            run_concurrent(adapter, count, body, raw).await;
        }

        "models" => {
            let list = adapter.list_models().await;
            println!("{}", serde_json::to_string(&list).unwrap());
        }

        "model" if parts.len() == 2 => {
            if let Some(model) = adapter.get_model(parts[1]).await {
                println!("{}", serde_json::to_string(&model).unwrap());
            } else {
                println!("null");
            }
        }

        "source" if parts.len() == 2 => {
            let file = parts[1];
            if !Path::new(file).exists() {
                eprintln!("[Error] File not found: {}", file);
                return Ok(false);
            }
            println!("[Running script: {}]", file);
            let content = std::fs::read_to_string(file)?;
            for script_line in content.lines() {
                let script_line = script_line.trim();
                if script_line.is_empty() || script_line.starts_with('#') {
                    continue;
                }
                println!(">>> {}", script_line);
                if Box::pin(handle_line(script_line, adapter)).await? {
                    return Ok(true);
                }
            }
            println!("[Script complete]");
        }

        "quit" | "exit" => {
            println!("[Quit]");
            return Ok(true);
        }

        _ => {
            println!(
                "[Unknown command: {}] Available: chat | raw | compare | concurrent | models | model | status | source | quit",
                cmd
            );
        }
    }

    Ok(false)
}

fn load_json(file: &str) -> anyhow::Result<Vec<u8>> {
    let path = Path::new(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }
    Ok(std::fs::read(path)?)
}

/// Consume a stream, applying the given function to each chunk
async fn consume_stream<F>(stream: StreamResponse, mut f: F)
where
    F: FnMut(Bytes),
{
    let mut stream = stream;
    while let Some(res) = stream.next().await {
        match res {
            Ok(bytes) => f(bytes),
            Err(e) => {
                eprintln!("\n[Stream error] {}", e);
                break;
            }
        }
    }
}

/// Print a streaming response
async fn print_stream(stream: &mut StreamResponse, raw: bool) {
    let mut stdout = io::stdout();
    while let Some(res) = stream.next().await {
        match res {
            Ok(bytes) => {
                if raw {
                    print!("{}", String::from_utf8_lossy(&bytes));
                    stdout.flush().unwrap();
                } else {
                    print_stream_chunk(&bytes);
                }
            }
            Err(e) => {
                eprintln!("\n[Stream error] {}", e);
                break;
            }
        }
    }
    if !raw {
        println!();
    }
}

/// Print a summary of a single converted chunk
fn print_stream_chunk(bytes: &Bytes) {
    let text = String::from_utf8_lossy(bytes);
    let json_str = text
        .strip_prefix("data: ")
        .and_then(|s| s.strip_suffix("\n\n"))
        .unwrap_or(&text);

    let v: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(val) => val,
        Err(_) => {
            print!("{}", text);
            return;
        }
    };

    let choice = v.get("choices").and_then(|c| c.get(0));
    let delta = choice.and_then(|c| c.get("delta"));
    let content = delta
        .and_then(|d| d.get("content"))
        .and_then(|c| c.as_str());
    let reasoning = delta
        .and_then(|d| d.get("reasoning_content"))
        .and_then(|c| c.as_str());
    let tool_calls = delta.and_then(|d| d.get("tool_calls"));
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str());
    let usage = v.get("usage");

    if choice.is_none() || usage.is_some() {
        if let Some(u) = usage {
            println!("[usage] {}", u);
            return;
        }
    }

    let mut parts = Vec::new();
    if let Some(c) = content {
        parts.push(format!("content={:?}", c));
    }
    if let Some(r) = reasoning {
        parts.push(format!("reasoning={:?}", r));
    }
    if let Some(t) = tool_calls {
        parts.push(format!(
            "tool_calls={}",
            serde_json::to_string(t).unwrap_or_default()
        ));
    }
    if let Some(f) = finish {
        parts.push(format!("finish={}", f));
    }

    if !parts.is_empty() {
        println!("[chunk] {}", parts.join(" | "));
    }
}

/// Execute concurrent requests
async fn run_concurrent(adapter: &OpenAIAdapter, count: usize, body: Vec<u8>, raw: bool) {
    let start = std::time::Instant::now();

    let futures: Vec<_> = (0..count)
        .map(|i| {
            let body = body.clone();
            async move {
                let req_start = std::time::Instant::now();
                let rid = demo_req_id();

                let req = match serde_json::from_slice::<ChatCompletionsRequest>(&body) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[Request {} parse failed] {}", i, e);
                        return (i, false, String::new(), req_start.elapsed());
                    }
                };

                let result = match adapter.chat_completions(req, &rid).await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[Request {} failed] {}", i, e);
                        return (i, false, String::new(), req_start.elapsed());
                    }
                };

                let (ok, output) = match result.data {
                    ChatOutput::Stream(mut s) => {
                        use futures::StreamExt;
                        let mut output = String::new();
                        let mut ok = true;
                        while let Some(chunk) = s.next().await {
                            match chunk {
                                Ok(c) => {
                                    if raw {
                                        output.push_str(&serde_json::to_string(&c).unwrap());
                                    } else if let Some(choice) = c.choices.first() {
                                        if let Some(ref content) = choice.delta.content {
                                            output.push_str(content);
                                        }
                                        if let Some(ref reasoning) = choice.delta.reasoning_content
                                        {
                                            if !output.is_empty() {
                                                output.push(' ');
                                            }
                                            output.push_str(reasoning);
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("\n[Request {} stream error] {}", i, e);
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        (ok, output)
                    }
                    ChatOutput::Json(json) => {
                        let output = if raw {
                            serde_json::to_string(&json).unwrap_or_default()
                        } else {
                            let mut parts = Vec::new();
                            if let Some(c) = json
                                .choices
                                .first()
                                .and_then(|c| c.message.content.as_deref())
                            {
                                parts.push(c.to_string());
                            }
                            if let Some(r) = json
                                .choices
                                .first()
                                .and_then(|c| c.message.reasoning_content.as_deref())
                            {
                                parts.push(r.to_string());
                            }
                            parts.join(" ")
                        };
                        (true, output)
                    }
                };
                (i, ok, output, req_start.elapsed())
            }
        })
        .collect();

    let results = join_all(futures).await;
    let total_elapsed = start.elapsed();

    println!("\n[Concurrent Results]");
    let success_count = results.iter().filter(|(_, ok, _, _)| *ok).count();
    for (i, ok, output, elapsed) in results {
        let preview: String = output.chars().take(80).collect();
        let status = if ok { "ok" } else { "fail" };
        println!(
            "  [Request {:2}] {} | {:>12?} | {}",
            i,
            status,
            elapsed,
            if preview.is_empty() {
                "(no output)".to_string()
            } else {
                format!("{}...", output.replace('\n', " "))
            }
        );
    }
    println!(
        "  Total: {}/{} succeeded | elapsed {:?}",
        success_count, count, total_elapsed
    );
}
