use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, Sender},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuFaultInjection {
    SurfaceLost,
    DeviceLost,
    OutOfMemory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuFault {
    DeviceLost,
    OutOfMemory,
    Internal,
    Validation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuFaultAction {
    ReinitializeDevice,
    FatalOutOfMemory,
    FatalValidation,
}

pub(crate) fn gpu_fault_action(fault: GpuFault) -> GpuFaultAction {
    match fault {
        GpuFault::DeviceLost | GpuFault::Internal => GpuFaultAction::ReinitializeDevice,
        GpuFault::OutOfMemory => GpuFaultAction::FatalOutOfMemory,
        GpuFault::Validation => GpuFaultAction::FatalValidation,
    }
}

pub(crate) struct GpuFaultMonitor {
    sender: Sender<GpuFault>,
    receiver: Receiver<GpuFault>,
    generation: Arc<AtomicU64>,
}

impl GpuFaultMonitor {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn install(&self, device: &wgpu::Device) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let active_generation = Arc::clone(&self.generation);
        let device_lost_sender = self.sender.clone();
        device.set_device_lost_callback(move |reason, message| {
            if active_generation.load(Ordering::Acquire) != generation
                || reason == wgpu::DeviceLostReason::Destroyed
            {
                return;
            }
            log::error!("wgpu device lost ({reason:?}): {message}");
            let _ = device_lost_sender.send(GpuFault::DeviceLost);
        });

        let active_generation = Arc::clone(&self.generation);
        let uncaptured_sender = self.sender.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            if active_generation.load(Ordering::Acquire) != generation {
                return;
            }
            let fault = match error {
                wgpu::Error::OutOfMemory { .. } => GpuFault::OutOfMemory,
                wgpu::Error::Validation { .. } => GpuFault::Validation,
                wgpu::Error::Internal { .. } => GpuFault::Internal,
            };
            log::error!("wgpu uncaptured error: {error}");
            let _ = uncaptured_sender.send(fault);
        }));
    }

    pub(crate) fn inject(&self, injection: GpuFaultInjection) {
        let fault = match injection {
            GpuFaultInjection::SurfaceLost => return,
            GpuFaultInjection::DeviceLost => GpuFault::DeviceLost,
            GpuFaultInjection::OutOfMemory => GpuFault::OutOfMemory,
        };
        let _ = self.sender.send(fault);
    }

    pub(crate) fn take_action(&self) -> Option<GpuFaultAction> {
        self.receiver
            .try_iter()
            .map(gpu_fault_action)
            .max_by_key(|action| match action {
                GpuFaultAction::ReinitializeDevice => 0,
                GpuFaultAction::FatalValidation => 1,
                GpuFaultAction::FatalOutOfMemory => 2,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_loss_and_internal_errors_reinitialize_all_gpu_state() {
        assert_eq!(
            gpu_fault_action(GpuFault::DeviceLost),
            GpuFaultAction::ReinitializeDevice
        );
        assert_eq!(
            gpu_fault_action(GpuFault::Internal),
            GpuFaultAction::ReinitializeDevice
        );
    }

    #[test]
    fn out_of_memory_and_validation_fail_closed() {
        assert_eq!(
            gpu_fault_action(GpuFault::OutOfMemory),
            GpuFaultAction::FatalOutOfMemory
        );
        assert_eq!(
            gpu_fault_action(GpuFault::Validation),
            GpuFaultAction::FatalValidation
        );
    }

    #[test]
    fn queued_fatal_fault_takes_priority_over_reinitialization() {
        let monitor = GpuFaultMonitor::new();
        monitor.inject(GpuFaultInjection::DeviceLost);
        monitor.inject(GpuFaultInjection::OutOfMemory);
        assert_eq!(
            monitor.take_action(),
            Some(GpuFaultAction::FatalOutOfMemory)
        );
        assert_eq!(monitor.take_action(), None);
    }
}
