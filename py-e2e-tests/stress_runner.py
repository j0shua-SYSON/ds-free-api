#!/usr/bin/env python3
"""Stress test entry point -- multi-iteration concurrency across all basic/ + repair/ scenarios

Safe concurrency = max(1, account count / 2); stress default = safe concurrency + 1
"""

import argparse
import json
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime
from typing import Any

from config import load_config
from runner import (
    load_scenarios, run_openai, run_anthropic,
    format_duration, print_report,
)
from openai import OpenAI
from anthropic import Anthropic
import httpx

def main():
    config = load_config()
    safe = config["safe_concurrency"]
    api_key = config["api_key"]
    stress_parallel = safe + 1

    parser = argparse.ArgumentParser(description="End-to-end stress test")
    parser.add_argument("--iterations", type=int, default=3, help="iterations per scenario (default: 3)")
    parser.add_argument("--parallel", type=int, default=stress_parallel, help=f"parallelism (default: {stress_parallel})")
    parser.add_argument("--models", type=str, nargs="*", default=None, help="model filter")
    parser.add_argument("--filter", type=str, nargs="*", default=None, help="scenario name keyword filter (multiple values separated by spaces)")
    parser.add_argument("--report", type=str, default=None, help="output JSON report path")
    parser.add_argument("--show-output", action="store_true", help="display model output content")
    args = parser.parse_args()

    # load all scenarios
    basic_oai = load_scenarios("scenarios/basic", "openai", args.filter)
    basic_anth = load_scenarios("scenarios/basic", "anthropic", args.filter)
    repair_sc = load_scenarios("scenarios/repair", None, args.filter)
    all_scenarios = basic_oai + basic_anth + repair_sc

    models = args.models or ["deepseek-default", "deepseek-expert"]

    port = config["port"]
    oai_client = OpenAI(base_url=f"http://127.0.0.1:{port}/v1", api_key=api_key)
    anth_client = Anthropic(
        base_url=f"http://127.0.0.1:{port}/anthropic", api_key=api_key,
        default_headers={"Authorization": f"Bearer {api_key}"},
        http_client=httpx.Client(timeout=120),
    )

    total_scenarios = len(all_scenarios)
    total_requests = total_scenarios * len(models) * args.iterations

    print(f"\nEnd-to-end stress test")
    print(f"  Scenarios: {total_scenarios} (basic + repair)")
    print(f"  Models: {', '.join(models)}")
    print(f"  Iterations: {args.iterations} per scenario per model")
    print(f"  Parallel: {args.parallel}")
    print(f"  Total: {total_requests} requests\n")

    tasks: list[tuple[str, str, dict, int]] = []
    for model in models:
        for sc in all_scenarios:
            for i in range(args.iterations):
                tasks.append((sc["endpoint"], model, sc, i))

    all_results: list[dict[str, Any]] = [None] * len(tasks)  # type: ignore[list-item]

    start_total = time.time()
    with ThreadPoolExecutor(max_workers=args.parallel) as executor:
        def run_task(endpoint: str, model: str, sc: dict, _idx: int) -> tuple[int, dict]:
            if endpoint == "openai":
                result = run_openai(oai_client, sc, model)
            else:
                result = run_anthropic(anth_client, sc, model)
            return (_idx, result)

        ep_label = {"openai": "OAI", "anthropic": "ANT"}
        task_labels: dict[int, str] = {}
        for i, (ep, model, sc, it) in enumerate(tasks):
            task_labels[i] = f"{ep_label.get(ep, '?')} | {sc['name']} | {model} | iter-{it + 1}"

        future_map = {}
        for i, (ep, model, sc, _) in enumerate(tasks):
            future = executor.submit(run_task, ep, model, sc, i)
            future_map[future] = i

        done = 0
        passed = 0
        for future in as_completed(future_map):
            idx = future_map[future]
            _, result = future.result()
            all_results[idx] = result
            done += 1
            if result["passed"]:
                passed += 1
            label = task_labels[idx]
            status = "✓" if result["passed"] else "✗"
            err = f" | {result['error'][:60]}" if result["error"] else ""
            print(f"  [{done}/{total_requests}] {status} | {label} | {result['duration']:.1f}s{err}")
            if args.show_output:
                from runner import _print_output
                _print_output(result)

    total_duration = time.time() - start_total
    print(f"\n  Total duration: {format_duration(total_duration)}")

    report = print_report(all_results, "End-to-end stress test report", args.parallel)
    report["total_duration"] = round(total_duration, 1)

    if args.report:
        with open(args.report, "w", encoding="utf-8") as f:
            json.dump({
                "suite": "stress",
                "started_at": datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
                "config": {
                    "iterations": args.iterations,
                    "parallel": args.parallel,
                    "models": models,
                    "accounts": config["accounts"],
                },
                "summary": report,
                "results": all_results,
            }, f, ensure_ascii=False, indent=2)
        print(f"  Report written to: {args.report}")

    sys.exit(0 if report["failed"] == 0 else 1)


if __name__ == "__main__":
    main()
