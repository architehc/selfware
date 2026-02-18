use domain::WorkItem;
use std::path::Path;

fn main() {
    let item = WorkItem::new("bootstrap-1", "scaffold mega workspace");
    let exists = storage::storage_ready(Path::new("."));
    println!("{} | storage_ready={}", item.title, exists);
}
