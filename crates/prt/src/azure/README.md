# Source-path compatibility

This directory is not a runtime Azure integration boundary. The implementation lives in `../integrations/azure/`.

`features/update_pull_request.rs` contains a source-level regression test that still reads `../azure/pull_requests.rs` with `include_str!`. The `pull_requests.rs` entry here is therefore a Git symlink to the real integration source. It preserves that existing test without duplicating or weakening the implementation. A future focused test cleanup can replace the source-path assertion with behavioral coverage and remove this compatibility path.
