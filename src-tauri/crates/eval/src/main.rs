use chrono::Utc;
use noesis_eval::run_evaluation_pipeline;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let include_draft = args.iter().any(|a| a == "--include-draft") || dry_run;

    let questions_dir = args
        .iter()
        .position(|a| a == "--questions")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("eval/questions"));

    let ts = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let out_dir = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("eval/out/{}", ts)));

    match run_evaluation_pipeline(&questions_dir, &out_dir, dry_run, include_draft) {
        Ok(report) => {
            println!(
                "Noesis Eval completed ({} questions, dry_run={}).",
                report.total_questions, report.dry_run
            );
            println!(
                "Noesis coverage: {:.1}% | Baseline coverage: {:.1}% | Verification rate: {:.1}%",
                report.overall_noesis_coverage * 100.0,
                report.overall_baseline_coverage * 100.0,
                report.overall_verification_rate * 100.0
            );
            println!("Artifacts written to: {}", out_dir.display());
        }
        Err(e) => {
            eprintln!("Noesis Eval failed: {}", e);
            std::process::exit(1);
        }
    }
}
