use super::args::{McpArgs, McpTarget};
use super::error::{CliError, Result};

/// Serve MCP on stdin and stdout against the running app or a headless session held to any roots given.
pub(crate) fn mcp(a: &McpArgs) -> Result {
    let backend: Box<dyn forja_mcp::Backend> = match a.target() {
        McpTarget::Connect { addr, lazy: true, launch } => Box::new(forja_mcp::Remote::lazy(&addr, launch)),
        McpTarget::Connect { addr, .. } => match forja_mcp::Remote::connect(&addr) {
            Ok(r) => Box::new(r),
            Err(source) => return Err(CliError::McpConnect { addr, source }),
        },
        McpTarget::Headless { read_root, write_root } => {
            let roots = forja_engine::file_access::AutomationRoots::new(
                read_root.as_deref().map(std::path::Path::new),
                write_root.as_deref().map(std::path::Path::new),
            )
            .map_err(CliError::AutomationRoots)?;
            Box::new(forja_mcp::Headless::default().with_automation_roots(roots))
        }
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    forja_mcp::Server::new(backend).serve(stdin.lock(), stdout.lock())?;
    Ok(())
}
