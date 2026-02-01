use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use fc_common::CiError;
use fc_common::config::EvaluatorConfig;
use fc_common::error::Result;
use fc_common::models::JobsetInput;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NixJob {
    pub name: String,
    #[serde(alias = "drvPath")]
    pub drv_path: String,
    pub system: Option<String>,
    pub outputs: Option<HashMap<String, String>>,
    #[serde(alias = "inputDrvs")]
    pub input_drvs: Option<HashMap<String, serde_json::Value>>,
    pub constituents: Option<Vec<String>>,
}

/// An error reported by nix-eval-jobs for a single job.
#[derive(Debug, Clone, Deserialize)]
struct NixEvalError {
    #[serde(alias = "attr")]
    name: Option<String>,
    error: String,
}

/// Result of evaluating nix expressions.
pub struct EvalResult {
    pub jobs: Vec<NixJob>,
    pub error_count: usize,
}

/// Parse nix-eval-jobs output lines into jobs and error counts.
/// Extracted as a testable function from the inline parsing loops.
pub fn parse_eval_output(stdout: &str) -> EvalResult {
    let mut jobs = Vec::new();
    let mut error_count = 0;

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line)
            && parsed.get("error").is_some() {
                if let Ok(eval_err) = serde_json::from_str::<NixEvalError>(line) {
                    let name = eval_err.name.as_deref().unwrap_or("<unknown>");
                    tracing::warn!(
                        job = name,
                        "nix-eval-jobs reported error: {}",
                        eval_err.error
                    );
                    error_count += 1;
                }
                continue;
            }

        match serde_json::from_str::<NixJob>(line) {
            Ok(job) => jobs.push(job),
            Err(e) => {
                tracing::warn!("Failed to parse nix-eval-jobs line: {e}");
            }
        }
    }

    EvalResult { jobs, error_count }
}

/// Evaluate nix expressions and return discovered jobs.
/// If flake_mode is true, uses nix-eval-jobs with --flake flag.
/// If flake_mode is false, evaluates a legacy expression file.
#[tracing::instrument(skip(config, inputs), fields(flake_mode, nix_expression))]
pub async fn evaluate(
    repo_path: &Path,
    nix_expression: &str,
    flake_mode: bool,
    timeout: Duration,
    config: &EvaluatorConfig,
    inputs: &[JobsetInput],
) -> Result<EvalResult> {
    if flake_mode {
        evaluate_flake(repo_path, nix_expression, timeout, config, inputs).await
    } else {
        evaluate_legacy(repo_path, nix_expression, timeout, config, inputs).await
    }
}

#[tracing::instrument(skip(config, inputs))]
async fn evaluate_flake(
    repo_path: &Path,
    nix_expression: &str,
    timeout: Duration,
    config: &EvaluatorConfig,
    inputs: &[JobsetInput],
) -> Result<EvalResult> {
    let flake_ref = format!("{}#{}", repo_path.display(), nix_expression);

    

    tokio::time::timeout(timeout, async {
        let mut cmd = tokio::process::Command::new("nix-eval-jobs");
        cmd.arg("--flake").arg(&flake_ref);

        if config.restrict_eval {
            cmd.args(["--option", "restrict-eval", "true"]);
        }
        if !config.allow_ifd {
            cmd.args(["--option", "allow-import-from-derivation", "false"]);
        }
        for input in inputs {
            if input.input_type == "git" {
                cmd.args(["--override-input", &input.name, &input.value]);
            }
        }

        let output = cmd.output().await;

        match output {
            Ok(out) if out.status.success() || !out.stdout.is_empty() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let result = parse_eval_output(&stdout);

                if result.error_count > 0 {
                    tracing::warn!(
                        error_count = result.error_count,
                        "nix-eval-jobs reported errors for some jobs"
                    );
                }

                Ok(result)
            }
            _ => {
                tracing::info!("nix-eval-jobs unavailable, falling back to nix eval");
                let jobs = evaluate_with_nix_eval(repo_path, nix_expression).await?;
                Ok(EvalResult {
                    jobs,
                    error_count: 0,
                })
            }
        }
    })
    .await
    .map_err(|_| CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}")))?
}

/// Legacy (non-flake) evaluation: import the nix expression file and evaluate it.
#[tracing::instrument(skip(config, inputs))]
async fn evaluate_legacy(
    repo_path: &Path,
    nix_expression: &str,
    timeout: Duration,
    config: &EvaluatorConfig,
    inputs: &[JobsetInput],
) -> Result<EvalResult> {
    let expr_path = repo_path.join(nix_expression);

    

    tokio::time::timeout(timeout, async {
        // Try nix-eval-jobs without --flake for legacy expressions
        let mut cmd = tokio::process::Command::new("nix-eval-jobs");
        cmd.arg(&expr_path);

        if config.restrict_eval {
            cmd.args(["--option", "restrict-eval", "true"]);
        }
        if !config.allow_ifd {
            cmd.args(["--option", "allow-import-from-derivation", "false"]);
        }
        for input in inputs {
            if input.input_type == "string" || input.input_type == "path" {
                cmd.args(["--arg", &input.name, &input.value]);
            }
        }

        let output = cmd.output().await;

        match output {
            Ok(out) if out.status.success() || !out.stdout.is_empty() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                Ok(parse_eval_output(&stdout))
            }
            _ => {
                // Fallback: nix eval on the legacy import
                tracing::info!("nix-eval-jobs unavailable for legacy expr, using nix-instantiate");
                let output = tokio::process::Command::new("nix-instantiate")
                    .arg(&expr_path)
                    .arg("--strict")
                    .arg("--json")
                    .output()
                    .await
                    .map_err(|e| CiError::NixEval(format!("nix-instantiate failed: {e}")))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(CiError::NixEval(format!(
                        "nix-instantiate failed: {stderr}"
                    )));
                }

                let stdout = String::from_utf8_lossy(&output.stdout);
                // nix-instantiate --json outputs the derivation path(s)
                let drv_paths: Vec<String> = serde_json::from_str(&stdout).unwrap_or_default();
                let jobs: Vec<NixJob> = drv_paths
                    .into_iter()
                    .enumerate()
                    .map(|(i, drv_path)| NixJob {
                        name: format!("job-{i}"),
                        drv_path,
                        system: None,
                        outputs: None,
                        input_drvs: None,
                        constituents: None,
                    })
                    .collect();

                Ok(EvalResult {
                    jobs,
                    error_count: 0,
                })
            }
        }
    })
    .await
    .map_err(|_| CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}")))?
}

async fn evaluate_with_nix_eval(repo_path: &Path, nix_expression: &str) -> Result<Vec<NixJob>> {
    let flake_ref = format!("{}#{}", repo_path.display(), nix_expression);

    let output = tokio::process::Command::new("nix")
        .args(["eval", "--json", &flake_ref])
        .output()
        .await
        .map_err(|e| CiError::NixEval(format!("Failed to run nix eval: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CiError::NixEval(format!("nix eval failed: {stderr}")));
    }

    // Parse the JSON output - expecting an attrset of name -> derivation
    let stdout = String::from_utf8_lossy(&output.stdout);
    let attrs: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| CiError::NixEval(format!("Failed to parse nix eval output: {e}")))?;

    let mut jobs = Vec::new();
    if let serde_json::Value::Object(map) = attrs {
        for (name, _value) in map {
            // Get derivation path via nix derivation show
            let drv_ref = format!("{}#{}.{}", repo_path.display(), nix_expression, name);
            let drv_output = tokio::process::Command::new("nix")
                .args(["derivation", "show", &drv_ref])
                .output()
                .await
                .map_err(|e| {
                    CiError::NixEval(format!("Failed to get derivation for {name}: {e}"))
                })?;

            if drv_output.status.success() {
                let drv_stdout = String::from_utf8_lossy(&drv_output.stdout);
                if let Ok(drv_json) = serde_json::from_str::<serde_json::Value>(&drv_stdout)
                    && let Some((drv_path, drv_val)) =
                        drv_json.as_object().and_then(|o| o.iter().next())
                    {
                        let system = drv_val
                            .get("system")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        jobs.push(NixJob {
                            name: name.clone(),
                            drv_path: drv_path.clone(),
                            system,
                            outputs: None,
                            input_drvs: None,
                            constituents: None,
                        });
                    }
            }
        }
    }

    Ok(jobs)
}
