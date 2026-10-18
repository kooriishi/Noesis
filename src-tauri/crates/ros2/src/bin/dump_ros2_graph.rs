use noesis_ros2::Ros2Adapter;
use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: dump_ros2_graph <workspace_root>");
        std::process::exit(1);
    }

    let root = PathBuf::from(&args[1]);
    if !root.exists() {
        eprintln!("Error: workspace path does not exist: {}", root.display());
        std::process::exit(1);
    }

    let adapter = Ros2Adapter::new();
    match adapter.analyze_workspace("cli_workspace", &root) {
        Ok((batch, summary)) => {
            let output = serde_json::json!({
                "workspace": root.to_string_lossy(),
                "entity_count": batch.entities.len(),
                "edge_count": batch.edges.len(),
                "summary": summary,
                "entities": batch.entities,
                "edges": batch.edges,
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
            );
        }
        Err(err) => {
            eprintln!("Error analyzing workspace: {}", err);
            std::process::exit(1);
        }
    }
}
