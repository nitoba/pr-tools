# Source compatibility fixture

This directory is not a runtime Azure integration module. The implementation lives in `../integrations/azure/`.

`features/update_pull_request.rs` currently contains a source-level regression test that reads `../azure/pull_requests.rs` with `include_str!`. The file here reuses the exact implementation blob so that the architectural move does not change behavior or force an unrelated rewrite of that regression test. A future focused test cleanup can replace the source inspection with behavioral assertions and remove this compatibility directory.
