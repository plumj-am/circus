use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};

use crate::error::ApiError;
use crate::state::AppState;

async fn build_badge(
    State(state): State<AppState>,
    Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    // Find the project
    let project = fc_common::repo::projects::get_by_name(&state.pool, &project_name)
        .await
        .map_err(ApiError)?;

    // Find the jobset
    let jobsets = fc_common::repo::jobsets::list_for_project(&state.pool, project.id, 1000, 0)
        .await
        .map_err(ApiError)?;

    let jobset = jobsets.iter().find(|j| j.name == jobset_name);
    let jobset = match jobset {
        Some(j) => j,
        None => {
            return Ok(shield_svg("build", "not found", "#9f9f9f").into_response());
        }
    };

    // Get latest evaluation
    let eval = fc_common::repo::evaluations::get_latest(&state.pool, jobset.id)
        .await
        .map_err(ApiError)?;

    let eval = match eval {
        Some(e) => e,
        None => {
            return Ok(shield_svg("build", "no evaluations", "#9f9f9f").into_response());
        }
    };

    // Find the build for this job
    let builds = fc_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
        .await
        .map_err(ApiError)?;

    let build = builds.iter().find(|b| b.job_name == job_name);

    let (label, color) = match build {
        Some(b) => match b.status {
            fc_common::BuildStatus::Completed => ("passing", "#4c1"),
            fc_common::BuildStatus::Failed => ("failing", "#e05d44"),
            fc_common::BuildStatus::Running => ("building", "#dfb317"),
            fc_common::BuildStatus::Pending => ("queued", "#dfb317"),
            fc_common::BuildStatus::Cancelled => ("cancelled", "#9f9f9f"),
        },
        None => ("not found", "#9f9f9f"),
    };

    Ok((
        StatusCode::OK,
        [
            ("content-type", "image/svg+xml"),
            ("cache-control", "no-cache, no-store, must-revalidate"),
        ],
        shield_svg("build", label, color),
    )
        .into_response())
}

/// Latest successful build redirect
async fn latest_build(
    State(state): State<AppState>,
    Path((project_name, jobset_name, job_name)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let project = fc_common::repo::projects::get_by_name(&state.pool, &project_name)
        .await
        .map_err(ApiError)?;

    let jobsets = fc_common::repo::jobsets::list_for_project(&state.pool, project.id, 1000, 0)
        .await
        .map_err(ApiError)?;

    let jobset = jobsets.iter().find(|j| j.name == jobset_name);
    let jobset = match jobset {
        Some(j) => j,
        None => {
            return Ok((StatusCode::NOT_FOUND, "Jobset not found").into_response());
        }
    };

    let eval = fc_common::repo::evaluations::get_latest(&state.pool, jobset.id)
        .await
        .map_err(ApiError)?;

    let eval = match eval {
        Some(e) => e,
        None => {
            return Ok((StatusCode::NOT_FOUND, "No evaluations found").into_response());
        }
    };

    let builds = fc_common::repo::builds::list_for_evaluation(&state.pool, eval.id)
        .await
        .map_err(ApiError)?;

    let build = builds.iter().find(|b| b.job_name == job_name);
    match build {
        Some(b) => Ok(axum::Json(b.clone()).into_response()),
        None => Ok((StatusCode::NOT_FOUND, "Build not found").into_response()),
    }
}

fn shield_svg(subject: &str, status: &str, color: &str) -> String {
    let subject_width = subject.len() * 7 + 10;
    let status_width = status.len() * 7 + 10;
    let total_width = subject_width + status_width;
    let subject_x = subject_width / 2;
    let status_x = subject_width + status_width / 2;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total_width}\" height=\"20\">\n"
    ));
    svg.push_str("  <linearGradient id=\"b\" x2=\"0\" y2=\"100%\">\n");
    svg.push_str("    <stop offset=\"0\" stop-color=\"#bbb\" stop-opacity=\".1\"/>\n");
    svg.push_str("    <stop offset=\"1\" stop-opacity=\".1\"/>\n");
    svg.push_str("  </linearGradient>\n");
    svg.push_str("  <mask id=\"a\">\n");
    svg.push_str(&format!(
        "    <rect width=\"{total_width}\" height=\"20\" rx=\"3\" fill=\"#fff\"/>\n"
    ));
    svg.push_str("  </mask>\n");
    svg.push_str("  <g mask=\"url(#a)\">\n");
    svg.push_str(&format!(
        "    <rect width=\"{subject_width}\" height=\"20\" fill=\"#555\"/>\n"
    ));
    svg.push_str(&format!(
        "    <rect x=\"{subject_width}\" width=\"{status_width}\" height=\"20\" fill=\"{color}\"/>\n"
    ));
    svg.push_str(&format!(
        "    <rect width=\"{total_width}\" height=\"20\" fill=\"url(#b)\"/>\n"
    ));
    svg.push_str("  </g>\n");
    svg.push_str("  <g fill=\"#fff\" text-anchor=\"middle\" font-family=\"DejaVu Sans,Verdana,Geneva,sans-serif\" font-size=\"11\">\n");
    svg.push_str(&format!(
        "    <text x=\"{subject_x}\" y=\"15\" fill=\"#010101\" fill-opacity=\".3\">{subject}</text>\n"
    ));
    svg.push_str(&format!(
        "    <text x=\"{subject_x}\" y=\"14\">{subject}</text>\n"
    ));
    svg.push_str(&format!(
        "    <text x=\"{status_x}\" y=\"15\" fill=\"#010101\" fill-opacity=\".3\">{status}</text>\n"
    ));
    svg.push_str(&format!(
        "    <text x=\"{status_x}\" y=\"14\">{status}</text>\n"
    ));
    svg.push_str("  </g>\n");
    svg.push_str("</svg>");
    svg
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/job/{project}/{jobset}/{job}/shield", get(build_badge))
        .route("/job/{project}/{jobset}/{job}/latest", get(latest_build))
}
