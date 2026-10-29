//! `arcl query`: run a GraphQL document against the records.

use super::support::*;
use super::*;

/// The arguments of `arcl query`.
pub(super) struct QueryInput {
    pub(super) document: Option<String>,
    pub(super) file: Option<PathBuf>,
    pub(super) variables: Option<String>,
    pub(super) operation: Option<String>,
    pub(super) schema: bool,
}

/// Print the schema, or run the query and print the GraphQL response. A
/// response with errors exits 3 after printing.
pub(super) fn exec_query(input: QueryInput) -> anyhow::Result<()> {
    if input.schema {
        return write_output(Some(arcl_graphql::sdl().trim_end().to_owned()));
    }
    let request = request(input).map_err(ApplicationError::from_command)?;
    let database = open_database().map_err(ApplicationError::from_command)?;
    let response = arcl_graphql::execute(database, request);
    write_output(Some(
        serde_json::to_string_pretty(&response).context("writing the GraphQL response")?,
    ))?;
    if response.errors.is_empty() {
        Ok(())
    } else {
        Err(ApplicationError::query_errors(response.errors.len()).into())
    }
}

fn request(input: QueryInput) -> CResult<arcl_graphql::Request> {
    let document = match (input.document, input.file) {
        (Some(document), _) => document,
        (None, Some(path)) if path == Path::new("-") => read_stdin()?.unwrap_or_default(),
        (None, Some(path)) => {
            let bytes = fs::read(&path).map_err(|source| CommandError::ReadQuery { path: path.clone(), source })?;
            String::from_utf8(bytes).map_err(|_| CommandError::InvalidQuery {
                message: format!("query file `{}` is not valid UTF-8", path.display()),
            })?
        }
        (None, None) => return Err(CommandError::InvalidQuery { message: "pass a query or `--file`".to_owned() }),
    };
    let mut request = arcl_graphql::Request::new(document);
    if let Some(variables) = input.variables {
        let value = serde_json::from_str::<serde_json::Value>(&variables)
            .ok()
            .filter(serde_json::Value::is_object)
            .ok_or_else(|| CommandError::InvalidQuery { message: "`--variables` must be a JSON object".to_owned() })?;
        request = request.variables(arcl_graphql::Variables::from_json(value));
    }
    if let Some(operation) = input.operation {
        request = request.operation_name(operation);
    }
    Ok(request)
}
