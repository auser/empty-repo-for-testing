use asupersync::runtime::RuntimeHandle;

use crate::{
    config::AppConfig,
    error::PireResult,
    http::Client,
    input::{Invocation, InvocationMode},
};

pub async fn run_app(
    config: &AppConfig,
    invocation: Invocation,
    client: Client,
    _runtime_handle: RuntimeHandle,
) -> PireResult<()> {
    tracing::debug!(
        mode = ?invocation.mode(),
        print = invocation.print(),
        argument_count = invocation.args().len(),
        stdin_present = invocation.stdin().is_some(),
        input_bytes = invocation.input_bytes(),
        user_agent = client.user_agent(),
        request_timeout = ?client.request_timeout(),
        response_limits = ?client.response_limits(),
        server_host = %config.server.host,
        server_port = config.server.port,
        "application context initialized"
    );

    match invocation.mode() {
        InvocationMode::Interactive => {
            println!("Pire interactive mode is not implemented yet.");
        }
        InvocationMode::OneShot => {
            println!("Pire request processing is not implemented yet.");
        }
    }

    Ok(())
}
