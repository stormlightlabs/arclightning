+++
id = "arcl-c-01M6Y4N5W8JHZBNQB3AV4VY0DH"
title = "Build the GraphQL records filter with RecordFilterArgs"
status = "captured"
created-at = "2026-11-08T17:52:47.496Z"
+++

The GraphQL `records` query in crates/arcl-graphql/src/query.rs builds its `RecordFilter` with the same steps, in the same order, that `RecordFilterArgs::resolve` in arcl-store now runs for `arcl list` and the MCP `list` tool. Using it needs an error type that implements `From<DomainError>` and `From<StorageError>` and keeps the `extensions.code` that `storage_error` and `priorities` set. Found while adding the list filters to the MCP tool.
