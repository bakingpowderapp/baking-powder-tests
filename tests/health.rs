use std::process::Command;

#[test]
fn tree_is_healthy() {
    let output = Command::new("./health.sh").output().expect("wanted health.sh to run");
    assert!(output.status.success(), "{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}
