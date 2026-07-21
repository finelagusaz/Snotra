use crate::{
    RuntimeError, SurfaceAction,
    gpu::{GpuFaultAction, GpuFaultInjection, GpuFaultMonitor},
    is_renderable_extent, surface_action,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintOutcome {
    Presented,
    Skipped,
    SurfaceRecovered,
    DeviceRecovered,
}

pub(crate) struct EguiRenderer {
    window: tauri::Window,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    render_state: egui_wgpu::RenderState,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    fault_monitor: GpuFaultMonitor,
    injected_surface_lost: bool,
}

impl EguiRenderer {
    pub(crate) fn new(window: tauri::Window) -> Result<Self, RuntimeError> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| RuntimeError::GpuInitialization(error.to_string()))?;
        let render_state = pollster::block_on(egui_wgpu::RenderState::create(
            &egui_wgpu::WgpuConfiguration::default(),
            &instance,
            Some(&surface),
            egui_wgpu::RendererOptions::default(),
        ))
        .map_err(|error| RuntimeError::GpuInitialization(error.to_string()))?;

        let fault_monitor = GpuFaultMonitor::new();
        fault_monitor.install(&render_state.device);
        let mut renderer = Self {
            window,
            instance,
            surface,
            render_state,
            surface_config: None,
            fault_monitor,
            injected_surface_lost: false,
        };
        let size = renderer.window.inner_size()?;
        renderer.configure(size.width, size.height)?;
        Ok(renderer)
    }

    pub(crate) fn max_texture_side(&self) -> usize {
        self.render_state.device.limits().max_texture_dimension_2d as usize
    }

    pub(crate) fn inject_fault(&mut self, fault: GpuFaultInjection) {
        if fault == GpuFaultInjection::SurfaceLost {
            self.injected_surface_lost = true;
        } else {
            self.fault_monitor.inject(fault);
        }
    }

    pub(crate) fn configure(&mut self, width: u32, height: u32) -> Result<(), RuntimeError> {
        if !is_renderable_extent(width, height) {
            self.surface_config = None;
            return Ok(());
        }

        let mut config = self
            .surface
            .get_default_config(&self.render_state.adapter, width, height)
            .ok_or_else(|| {
                RuntimeError::GpuInitialization("surface has no supported configuration".to_owned())
            })?;
        config.format = self.render_state.target_format;
        // #532 SU1 G2: parity 論は egui-wgpu が UNORM 形式（gamma 空間 blend）を選ぶことに
        // 依存する。softbuffer の gamma 空間 CPU blend が的と一致するかを wgpu 撤去前に確定する。
        // is_srgb=true なら CPU blend の色空間設計を見直す（撤去は Task 6）。
        eprintln!(
            "SNOTRA_EGUI_TARGET_FORMAT format={:?} is_srgb={}",
            self.render_state.target_format,
            self.render_state.target_format.is_srgb()
        );
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = 1;
        self.surface.configure(&self.render_state.device, &config);
        self.surface_config = Some(config);
        Ok(())
    }

    pub(crate) fn paint(
        &mut self,
        context: &egui::Context,
        output: egui::FullOutput,
    ) -> Result<PaintOutcome, RuntimeError> {
        let Some(config) = self.surface_config.clone() else {
            return Ok(PaintOutcome::Skipped);
        };

        let _ = self.render_state.device.poll(wgpu::PollType::Poll);
        if let Some(action) = self.fault_monitor.take_action() {
            return match action {
                GpuFaultAction::ReinitializeDevice => {
                    self.reinitialize_gpu(config.width, config.height)?;
                    log::warn!("SNOTRA_EGUI_GPU_RECOVERY=device-reinitialized");
                    eprintln!("SNOTRA_EGUI_GPU_RECOVERY=device-reinitialized");
                    context.request_repaint();
                    Ok(PaintOutcome::DeviceRecovered)
                }
                GpuFaultAction::FatalOutOfMemory => Err(RuntimeError::GpuOutOfMemory),
                GpuFaultAction::FatalValidation => Err(RuntimeError::SurfaceValidation),
            };
        }
        if std::mem::take(&mut self.injected_surface_lost) {
            self.recreate_surface(config.width, config.height)?;
            log::warn!("SNOTRA_EGUI_GPU_RECOVERY=surface-recreated");
            eprintln!("SNOTRA_EGUI_GPU_RECOVERY=surface-recreated");
            context.request_repaint();
            return Ok(PaintOutcome::SurfaceRecovered);
        }

        let egui::FullOutput {
            platform_output: _,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output: _,
        } = output;
        let paint_jobs = context.tessellate(shapes, pixels_per_point);
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [config.width, config.height],
            pixels_per_point,
        };

        {
            let mut renderer = self.render_state.renderer.write();
            for (texture_id, image_delta) in &textures_delta.set {
                renderer.update_texture(
                    &self.render_state.device,
                    &self.render_state.queue,
                    *texture_id,
                    image_delta,
                );
            }
        }

        let current_surface_texture = self.surface.get_current_texture();
        match surface_action(&current_surface_texture) {
            SurfaceAction::Skip => {
                self.free_textures(&textures_delta.free);
                context.request_repaint_after(std::time::Duration::from_millis(16));
                return Ok(PaintOutcome::Skipped);
            }
            SurfaceAction::Reconfigure => {
                self.free_textures(&textures_delta.free);
                self.configure(config.width, config.height)?;
                context.request_repaint();
                return Ok(PaintOutcome::SurfaceRecovered);
            }
            SurfaceAction::Recreate => {
                self.free_textures(&textures_delta.free);
                self.recreate_surface(config.width, config.height)?;
                context.request_repaint();
                return Ok(PaintOutcome::SurfaceRecovered);
            }
            SurfaceAction::FatalValidation => {
                self.free_textures(&textures_delta.free);
                return Err(RuntimeError::SurfaceValidation);
            }
            SurfaceAction::Render | SurfaceAction::RenderThenReconfigure => {}
        }
        let (surface_texture, reconfigure_after_present) = match current_surface_texture {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            _ => unreachable!("non-renderable surface states returned before texture extraction"),
        };

        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder =
            self.render_state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("snotra-egui-encoder"),
                });

        let user_command_buffers = {
            let mut renderer = self.render_state.renderer.write();
            let user_command_buffers = renderer.update_buffers(
                &self.render_state.device,
                &self.render_state.queue,
                &mut encoder,
                &paint_jobs,
                &screen_descriptor,
            );
            {
                let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("snotra-egui-render-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &texture_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.047,
                                g: 0.055,
                                b: 0.071,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                renderer.render(
                    &mut render_pass.forget_lifetime(),
                    &paint_jobs,
                    &screen_descriptor,
                );
            }
            user_command_buffers
        };

        self.render_state.queue.submit(
            user_command_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        surface_texture.present();

        self.free_textures(&textures_delta.free);

        if reconfigure_after_present {
            self.configure(config.width, config.height)?;
            context.request_repaint();
        }
        Ok(PaintOutcome::Presented)
    }

    fn free_textures(&self, textures: &[egui::TextureId]) {
        let mut renderer = self.render_state.renderer.write();
        for texture_id in textures {
            renderer.free_texture(texture_id);
        }
    }

    fn recreate_surface(&mut self, width: u32, height: u32) -> Result<(), RuntimeError> {
        self.surface = self
            .instance
            .create_surface(self.window.clone())
            .map_err(|error| RuntimeError::GpuInitialization(error.to_string()))?;
        self.configure(width, height)
    }

    fn reinitialize_gpu(&mut self, width: u32, height: u32) -> Result<(), RuntimeError> {
        let surface = self
            .instance
            .create_surface(self.window.clone())
            .map_err(|error| RuntimeError::GpuInitialization(error.to_string()))?;
        let render_state = pollster::block_on(egui_wgpu::RenderState::create(
            &egui_wgpu::WgpuConfiguration::default(),
            &self.instance,
            Some(&surface),
            egui_wgpu::RendererOptions::default(),
        ))
        .map_err(|error| RuntimeError::GpuInitialization(error.to_string()))?;
        self.fault_monitor.install(&render_state.device);
        self.surface = surface;
        self.render_state = render_state;
        self.surface_config = None;
        self.configure(width, height)
    }
}

/// #532 SU1 G1: softbuffer が tao/wry 管理の tauri::Window に rwh 0.6 で束ねられる
/// ことをコンパイルで確定する。never-called。撤去済み wgpu の代替が成立する一次証拠。
#[allow(dead_code)]
fn _softbuffer_bind_check(window: tauri::Window) -> Result<(), softbuffer::SoftBufferError> {
    use std::num::NonZeroU32;
    let context = softbuffer::Context::new(window.clone())?;
    let mut surface = softbuffer::Surface::new(&context, window)?;
    surface.resize(NonZeroU32::new(1).unwrap(), NonZeroU32::new(1).unwrap())?;
    let mut buffer = surface.buffer_mut()?;
    buffer.fill(0x0028_2828);
    buffer.present()?;
    Ok(())
}
