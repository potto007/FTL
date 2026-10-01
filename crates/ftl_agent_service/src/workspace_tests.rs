use super::*;
use crate::tests::TestDirectory;
use ftl_agent_core::EmptyToolHost;

#[tokio::test]
async fn approved_workspace_reads_reject_escape_binary_and_oversized_data() {
    let fixture = TestDirectory::new();
    let root = fixture.0.join("workspace");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("hello.txt"), "hello workspace").unwrap();
    std::fs::write(fixture.0.join("outside.txt"), "outside").unwrap();
    std::fs::write(root.join("binary"), [255, 254]).unwrap();
    std::fs::write(root.join("large"), vec![b'a'; 65537]).unwrap();
    let host = WorkspaceHost::new(Some(root), Arc::new(EmptyToolHost)).unwrap();
    let result = host
        .call("test", "workspace_read", json!({"path":"hello.txt"}))
        .await
        .unwrap();
    assert_eq!(result["content"][0]["text"], "hello workspace");
    for path in ["../outside.txt", "binary", "large"] {
        assert!(
            host.call("test", "workspace_read", json!({"path":path}))
                .await
                .is_err(),
            "{path}"
        );
    }
    assert!(
        host.call(
            "test",
            "workspace_read",
            json!({"path":fixture.0.join("outside.txt")})
        )
        .await
        .is_err()
    );
    let disabled = WorkspaceHost::new(None, Arc::new(EmptyToolHost)).unwrap();
    assert!(disabled.tools().is_empty());
    assert!(
        disabled
            .call("test", "workspace_read", json!({"path":"hello.txt"}))
            .await
            .is_err()
    );
}
