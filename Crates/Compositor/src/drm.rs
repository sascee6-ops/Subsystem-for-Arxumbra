use std::time::Duration;

use smithay::{
    backend::{
        allocator::{
            Fourcc, Modifier,
            format::FormatSet,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEvent,
            compositor::FrameFlags,
            exporter::gbm::GbmFramebufferExporter,
            output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
        },
        egl::{EGLContext, EGLDevice, EGLDisplay},
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{
            ImportAll, ImportMem,
            element::{
                Kind,
                solid::SolidColorRenderElement,
                surface::WaylandSurfaceRenderElement,
            },
            gles::GlesRenderer,
        },
        session::{
            Event as SessionEvent, Session,
            libseat::LibSeatSession,
        },
        udev::{all_gpus, primary_gpu},
    },
    desktop::space::{SpaceRenderElements, space_render_elements},
    output::{Mode, Output, PhysicalProperties},
    reexports::{
        calloop::{
            EventLoop, LoopHandle,
            timer::{TimeoutAction, Timer},
        },
        drm::control::{Device, ModeTypeFlags, connector, crtc},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};

use crate::Smallvil;

type Allocator = GbmAllocator<DrmDeviceFd>;
type Exporter = GbmFramebufferExporter<DrmDeviceFd>;
type Scanout = DrmOutput<Allocator, Exporter, (), DrmDeviceFd>;

smithay::backend::renderer::element::render_elements! {
    Scene<R, E> where R: ImportAll + ImportMem;
    Space = SpaceRenderElements<R, E>,
    Cursor = SolidColorRenderElement,
}

type Frame = Scene<GlesRenderer, WaylandSurfaceRenderElement<GlesRenderer>>;

struct Surface {
    output: Output,
    scanout: Scanout,
}

pub struct Drm {
    handle: LoopHandle<'static, Smallvil>,
    session: LibSeatSession,
    manager: DrmOutputManager<Allocator, Exporter, (), DrmDeviceFd>,
    renderer: GlesRenderer,
    surfaces: Vec<Surface>,
    active: bool,
}

pub fn init(
    event_loop: &mut EventLoop<'static, Smallvil>,
    state: &mut Smallvil,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut session, notifier) = LibSeatSession::new()?;
    let seat = session.seat();

    let path = std::env::var_os("COMPOSITOR_DRM_DEVICE")
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| primary_gpu(&seat).ok().flatten())
        .or_else(|| all_gpus(&seat).ok().and_then(|gpus| gpus.into_iter().next()))
        .ok_or("no gpu")?;

    let fd = session.open(
        &path,
        OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
    )?;
    let fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, device_notifier) = DrmDevice::new(fd.clone(), true)?;
    let gbm = GbmDevice::new(fd)?;

    let display = unsafe { EGLDisplay::new(gbm.clone())? };
    let render_node = EGLDevice::device_for_display(&display)?
        .try_get_render_node()
        .ok()
        .flatten();
    let mut renderer = unsafe { GlesRenderer::new(EGLContext::new(&display)?)? };
    let render_formats = renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .filter(|format| render_node.is_some() || format.modifier == Modifier::Linear)
        .copied()
        .collect::<FormatSet>();

    let mut manager = DrmOutputManager::new(
        device,
        GbmAllocator::new(gbm.clone(), GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT),
        GbmFramebufferExporter::new(gbm.clone(), render_node.into()),
        Some(gbm),
        [
            Fourcc::Abgr2101010,
            Fourcc::Argb2101010,
            Fourcc::Abgr8888,
            Fourcc::Argb8888,
        ],
        render_formats,
    );

    let resources = manager.device().resource_handles()?;
    let mut used = Vec::<crtc::Handle>::new();
    let mut targets = Vec::new();
    for handle in resources.connectors() {
        let info = manager.device().get_connector(*handle, true)?;
        if info.state() != connector::State::Connected || info.modes().is_empty() {
            continue;
        }
        let free = info
            .encoders()
            .iter()
            .filter_map(|encoder| manager.device().get_encoder(*encoder).ok())
            .flat_map(|encoder| resources.filter_crtcs(encoder.possible_crtcs()))
            .find(|crtc| !used.contains(crtc));
        if let Some(crtc) = free {
            used.push(crtc);
            targets.push((info, crtc));
        }
    }
    if targets.is_empty() {
        return Err("no connected output".into());
    }

    let mut surfaces = Vec::new();
    for (info, crtc) in targets {
        let drm_mode = info.modes()[info
            .modes()
            .iter()
            .position(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
            .unwrap_or(0)];
        let mode = Mode::from(drm_mode);
        let (width, height) = info.size().unwrap_or((0, 0));
        let output = Output::new(
            format!("{}-{}", info.interface().as_str(), info.interface_id()),
            PhysicalProperties {
                size: (width as i32, height as i32).into(),
                subpixel: info.subpixel().into(),
                make: "Unknown".into(),
                model: "Unknown".into(),
                serial_number: "Unknown".into(),
            },
        );
        output.create_global::<Smallvil>(&state.display_handle);
        let position = (
            state
                .space
                .outputs()
                .fold(0, |sum, output| sum + state.space.output_geometry(output).unwrap().size.w),
            0,
        )
            .into();
        output.set_preferred(mode);
        output.change_current_state(Some(mode), None, None, Some(position));
        state.space.map_output(&output, position);

        let scanout = manager
            .lock()
            .initialize_output::<_, SolidColorRenderElement>(
                crtc,
                drm_mode,
                &[info.handle()],
                &output,
                None,
                &mut renderer,
                &DrmOutputRenderElements::default(),
            )
            .map_err(|error| format!("{error:?}"))?;
        surfaces.push(Surface { output, scanout });
    }

    let handle = event_loop.handle();

    let mut input = Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    input.udev_assign_seat(&seat).map_err(|()| "libinput seat")?;

    handle.insert_source(device_notifier, |event, _, state| match event {
        DrmEvent::VBlank(crtc) => state.frame_done(crtc),
        DrmEvent::Error(_) => {}
    })?;
    handle.insert_source(LibinputInputBackend::new(input.clone()), |event, _, state| {
        state.process_input_event(event);
        state.switch_vt();
    })?;
    handle.insert_source(notifier, move |event, _, state| match event {
        SessionEvent::PauseSession => {
            input.suspend();
            state.pause();
        }
        SessionEvent::ActivateSession => {
            input.resume().ok();
            state.resume();
        }
    })?;

    let crtcs = surfaces.iter().map(|surface| surface.scanout.crtc()).collect::<Vec<_>>();
    state.drm = Some(Drm {
        handle: handle.clone(),
        session,
        manager,
        renderer,
        surfaces,
        active: true,
    });
    crtcs
        .into_iter()
        .for_each(|crtc| drop(handle.insert_idle(move |state| state.render_surface(crtc))));

    Ok(())
}

impl Smallvil {
    fn pause(&mut self) {
        let drm = self.drm.as_mut().unwrap();
        drm.active = false;
        drm.manager.pause();
    }

    fn resume(&mut self) {
        let drm = self.drm.as_mut().unwrap();
        drm.active = true;
        drm.manager.lock().activate(false).unwrap();
        let crtcs = drm.surfaces.iter().map(|surface| surface.scanout.crtc()).collect::<Vec<_>>();
        crtcs.into_iter().for_each(|crtc| self.render_surface(crtc));
    }

    pub fn switch_vt(&mut self) {
        match (self.vt.take(), self.drm.as_mut()) {
            (Some(vt), Some(drm)) => drm.session.change_vt(vt).unwrap(),
            _ => {}
        }
    }

    fn frame_done(&mut self, crtc: crtc::Handle) {
        self.drm
            .as_mut()
            .unwrap()
            .surfaces
            .iter()
            .filter(|surface| surface.scanout.crtc() == crtc)
            .for_each(|surface| drop(surface.scanout.frame_submitted()));
        self.render_surface(crtc);
    }

    fn render_surface(&mut self, crtc: crtc::Handle) {
        let drm = self.drm.as_mut().unwrap();
        if !drm.active {
            return;
        }
        let surface = drm
            .surfaces
            .iter_mut()
            .find(|surface| surface.scanout.crtc() == crtc)
            .unwrap();

        let origin = self.space.output_geometry(&surface.output).unwrap().loc.to_f64();
        let pointer = self.seat.get_pointer().unwrap().current_location() - origin;
        let mut elements = vec![Frame::Cursor(SolidColorRenderElement::from_buffer(
            &self.cursor,
            (pointer.x as i32, pointer.y as i32),
            1.0,
            1.0,
            Kind::Cursor,
        ))];
        elements.extend(
            space_render_elements(&mut drm.renderer, [&self.space], &surface.output, 1.0)
                .unwrap()
                .into_iter()
                .map(Frame::Space),
        );

        let queued = surface
            .scanout
            .render_frame(&mut drm.renderer, &elements, [0.1, 0.1, 0.1, 1.0], FrameFlags::DEFAULT)
            .map(|frame| !frame.is_empty)
            .unwrap_or(false)
            && surface.scanout.queue_frame(()).is_ok();

        if !queued {
            drm.handle
                .insert_source(Timer::from_duration(Duration::from_millis(16)), move |_, _, state| {
                    state.render_surface(crtc);
                    TimeoutAction::Drop
                })
                .unwrap();
        }

        let output = surface.output.clone();
        self.space.elements().for_each(|window| {
            window.send_frame(
                &output,
                self.start_time.elapsed(),
                Some(Duration::ZERO),
                |_, _| Some(output.clone()),
            )
        });
        self.space.refresh();
        self.popups.cleanup();
        self.display_handle.flush_clients().ok();
    }
}
