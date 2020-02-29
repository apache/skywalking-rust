use crate::TracingContext;

///Report bridge defines the traits for the skywalking-report

/// Register implementation communicate with the SkyWalking OAP backend.
/// It does metadata register, traces report, and runtime status report or interaction.
pub trait Reporter {
    /// Return the registered service id
    /// If haven't registered successfully, return None.
    fn service_instance_id(&self) -> Option<i32>;

    /// Move the finished and inactive context to the reporter.
    /// The reporter should use async way to transport the data to the backend through HTTP, gRPC or SkyWalking forwarder.
    fn report_trace(&self, finished_context: TracingContext);
}
