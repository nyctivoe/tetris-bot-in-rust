mod common;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tetrisBot::data::Placement;

use common::{canonical_snapshot_cases, suggestions_for_case};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SnapshotResult {
    case: String,
    suggestions: Vec<Placement>,
}

#[test]
fn canonical_move_snapshots_match() {
    let actual = canonical_snapshot_cases()
        .into_iter()
        .map(|case| SnapshotResult {
            case: case.name.to_string(),
            suggestions: suggestions_for_case(&case),
        })
        .collect::<Vec<_>>();

    if std::env::var_os("UPDATE_MOVE_SNAPSHOTS").is_some() {
        std::fs::write(
            snapshot_path(),
            serde_json::to_string_pretty(&actual).expect("snapshot serialization must succeed"),
        )
        .expect("snapshot fixture must be writable");
        return;
    }

    let expected =
        serde_json::from_str::<Vec<SnapshotResult>>(include_str!("fixtures/move_snapshots.json"))
            .expect("snapshot fixture must be valid json");

    assert_eq!(actual, expected);
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("move_snapshots.json")
}
