#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

struct Dir {
    path: std::path::PathBuf,
    parent: Option<std::sync::Arc<Dir>>,
    row: usize,
    sum: std::sync::atomic::AtomicU64,
    open: std::sync::atomic::AtomicUsize,
}

struct Walk {
    queue: std::sync::Mutex<Vec<std::sync::Arc<Dir>>>,
    todo: std::sync::atomic::AtomicUsize,
    stop: std::sync::atomic::AtomicBool,
    live: Vec<std::sync::atomic::AtomicU64>,
}

type Done = std::sync::mpsc::Sender<(usize, std::path::PathBuf, u64, bool)>;

fn scan(job: &Walk, dir: &std::sync::Arc<Dir>, done: &Done) {
    for entries in [std::fs::read_dir(&dir.path)
        .ok()
        .filter(|_| {
            !["/proc", "/sys", "/dev"]
                .iter()
                .any(|root| dir.path.starts_with(root))
        })
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .and_then(|kind| match (kind.is_dir(), kind.is_file()) {
                    (true, _) => Some((Some(entry.path()), 0)),
                    (_, true) => entry.metadata().ok().map(|meta| (None, meta.len())),
                    _ => None,
                })
        })
        .collect::<Vec<_>>()]
    {
        for (own, kids) in [(
            entries.iter().map(|entry| entry.1).sum::<u64>(),
            entries
                .into_iter()
                .filter_map(|entry| entry.0)
                .map(|path| {
                    std::sync::Arc::new(Dir {
                        path,
                        parent: Some(dir.clone()),
                        row: dir.row,
                        sum: std::sync::atomic::AtomicU64::new(0),
                        open: std::sync::atomic::AtomicUsize::new(1),
                    })
                })
                .collect::<Vec<_>>(),
        )] {
            job.live
                .get(dir.row)
                .into_iter()
                .chain(std::iter::once(&dir.sum))
                .for_each(|total| {
                    total.fetch_add(own, std::sync::atomic::Ordering::AcqRel);
                });
            dir.open
                .fetch_add(kids.len(), std::sync::atomic::Ordering::AcqRel);
            job.todo
                .fetch_add(kids.len(), std::sync::atomic::Ordering::AcqRel);
            job.queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend(kids);
            std::iter::successors(Some(dir.clone()), |node| {
                (node.open.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1)
                    .then(|| {
                        for total in [node.sum.load(std::sync::atomic::Ordering::Acquire)] {
                            done.send((node.row, node.path.clone(), total, node.parent.is_none()))
                                .ok();
                            node.parent.iter().for_each(|parent| {
                                parent
                                    .sum
                                    .fetch_add(total, std::sync::atomic::Ordering::AcqRel);
                            });
                        }
                    })
                    .and_then(|()| node.parent.clone())
            })
            .for_each(drop);
            job.todo.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
}

fn walk(job: &Walk, done: &Done) {
    std::iter::from_fn(|| {
        (!job.stop.load(std::sync::atomic::Ordering::Relaxed))
            .then(|| {
                job.queue
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop()
            })
            .and_then(|next| match next {
                Some(dir) => Some(scan(job, &dir, done)),
                None => (job.todo.load(std::sync::atomic::Ordering::Acquire) > 0)
                    .then(|| std::thread::sleep(std::time::Duration::from_micros(300))),
            })
    })
    .for_each(drop);
}

fn main() {
    std::iter::once(
        (if cfg!(target_os = "windows") {
            std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap())
        } else if cfg!(target_os = "macos") {
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("Library/Application Support")
        } else {
            std::env::var_os("XDG_DATA_HOME").map_or_else(
                || std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share"),
                std::path::PathBuf::from,
            )
        })
        .join("Subsystem-for-Arxumbra"),
    )
    .inspect(|home| std::fs::create_dir_all(home).unwrap())
    .map(|home| {
        (
            redb::Database::create(home.join("projects.redb")).unwrap(),
            if cfg!(target_os = "linux") {
                "Linux-Subsystem-for-Arxumbra"
            } else if cfg!(target_os = "macos") {
                "macOS-Subsystem-for-Arxumbra"
            } else {
                "Windows-Subsystem-for-Arxumbra"
            },
        )
    })
    .for_each(|(db, title)| {
    bevy::app::App::new()
        .add_plugins((
            bevy::app::TaskPoolPlugin {
                task_pool_options: bevy::app::TaskPoolOptions {
                    min_total_threads: 1,
                    max_total_threads: usize::MAX,
                    io: bevy::app::TaskPoolThreadAssignmentPolicy {
                        min_threads: 1,
                        max_threads: 4,
                        percent: 0.25,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    async_compute: bevy::app::TaskPoolThreadAssignmentPolicy {
                        min_threads: 1,
                        max_threads: 4,
                        percent: 0.25,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                    compute: bevy::app::TaskPoolThreadAssignmentPolicy {
                        min_threads: 1,
                        max_threads: usize::MAX,
                        percent: 1.0,
                        on_thread_spawn: None,
                        on_thread_destroy: None,
                    },
                },
            },
            bevy::diagnostic::FrameCountPlugin,
            bevy::time::TimePlugin,
            bevy::transform::TransformPlugin,
            bevy::input::InputPlugin,
            bevy::a11y::AccessibilityPlugin,
            bevy::window::WindowPlugin {
                primary_window: Some(bevy::window::Window {
                    mode: bevy::window::WindowMode::BorderlessFullscreen(
                        bevy::window::MonitorSelection::Primary,
                    ),
                    title: String::from(title),
                    skip_taskbar: true,
                    present_mode: bevy::window::PresentMode::AutoVsync,
                    position: bevy::window::WindowPosition::Automatic,
                    resolution: bevy::window::WindowResolution::new(1280, 720),
                    name: None,
                    composite_alpha_mode: bevy::window::CompositeAlphaMode::Auto,
                    resize_constraints: bevy::window::WindowResizeConstraints {
                        min_width: 180.0,
                        min_height: 120.0,
                        max_width: f32::INFINITY,
                        max_height: f32::INFINITY,
                    },
                    resizable: true,
                    enabled_buttons: bevy::window::EnabledButtons {
                        minimize: true,
                        maximize: true,
                        close: true,
                    },
                    decorations: false,
                    transparent: false,
                    focused: true,
                    window_level: bevy::window::WindowLevel::Normal,
                    canvas: None,
                    fit_canvas_to_parent: false,
                    prevent_default_event_handling: true,
                    internal: bevy::window::InternalWindowState::default(),
                    ime_enabled: false,
                    ime_position: bevy::math::Vec2::ZERO,
                    window_theme: None,
                    visible: true,
                    clip_children: true,
                    desired_maximum_frame_latency: None,
                    recognize_pinch_gesture: false,
                    recognize_rotation_gesture: false,
                    recognize_doubletap_gesture: false,
                    recognize_pan_gesture: None,
                    movable_by_window_background: false,
                    fullsize_content_view: false,
                    has_shadow: false,
                    titlebar_shown: false,
                    titlebar_transparent: false,
                    titlebar_show_title: false,
                    titlebar_show_buttons: false,
                    borderless_game: true,
                    prefers_home_indicator_hidden: false,
                    prefers_status_bar_hidden: false,
                    preferred_screen_edges_deferring_system_gestures: bevy::window::ScreenEdge::None,
                }),
                primary_cursor_options: Some(bevy::window::CursorOptions {
                    visible: true,
                    grab_mode: bevy::window::CursorGrabMode::None,
                    hit_test: true,
                }),
                exit_condition: bevy::window::ExitCondition::OnAllClosed,
                close_when_requested: true,
            },
            bevy::asset::AssetPlugin {
                file_path: String::from("assets"),
                processed_file_path: String::from("imported_assets/Default"),
                watch_for_changes_override: None,
                use_asset_processor_override: None,
                mode: bevy::asset::AssetMode::Unprocessed,
                meta_check: bevy::asset::AssetMetaCheck::Always,
                unapproved_path_mode: bevy::asset::UnapprovedPathMode::Forbid,
            },
            bevy::winit::WinitPlugin { run_on_any_thread: false },
        ))
        .add_plugins((
            bevy::render::RenderPlugin {
                render_creation: bevy::render::settings::RenderCreation::Automatic(
                    Box::new(bevy::render::settings::WgpuSettings {
                        backends: Some(
                            bevy::render::settings::Backends::VULKAN
                                | bevy::render::settings::Backends::METAL
                                | bevy::render::settings::Backends::DX12,
                        ),
                        instance_flags: bevy::render::settings::InstanceFlags::empty(),
                        power_preference: bevy::render::settings::PowerPreference::LowPower,
                        device_label: None,
                        priority: bevy::render::settings::WgpuSettingsPriority::Functionality,
                        features: bevy::render::settings::WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                        disabled_features: None,
                        limits: bevy::render::settings::WgpuLimits {
                        max_texture_dimension_1d: 8192,
                        max_texture_dimension_2d: 8192,
                        max_texture_dimension_3d: 2048,
                        max_texture_array_layers: 256,
                        max_bind_groups: 4,
                        max_bind_groups_plus_vertex_buffers: 24,
                        max_bindings_per_bind_group: 1000,
                        max_dynamic_uniform_buffers_per_pipeline_layout: 8,
                        max_dynamic_storage_buffers_per_pipeline_layout: 4,
                        max_sampled_textures_per_shader_stage: 16,
                        max_samplers_per_shader_stage: 16,
                        max_storage_buffers_per_shader_stage: 8,
                        max_storage_textures_per_shader_stage: 4,
                        max_uniform_buffers_per_shader_stage: 12,
                        max_binding_array_elements_per_shader_stage: 0,
                        max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
                        max_binding_array_sampler_elements_per_shader_stage: 0,
                        max_uniform_buffer_binding_size: 64 << 10,
                        max_storage_buffer_binding_size: 128 << 20,
                        max_vertex_buffers: 8,
                        max_buffer_size: 256 << 20,
                        max_vertex_attributes: 16,
                        max_vertex_buffer_array_stride: 2048,
                        max_inter_stage_shader_variables: 16,
                        min_uniform_buffer_offset_alignment: 256,
                        min_storage_buffer_offset_alignment: 256,
                        max_color_attachments: 8,
                        max_color_attachment_bytes_per_sample: 32,
                        max_compute_workgroup_storage_size: 16384,
                        max_compute_invocations_per_workgroup: 256,
                        max_compute_workgroup_size_x: 256,
                        max_compute_workgroup_size_y: 256,
                        max_compute_workgroup_size_z: 64,
                        max_compute_workgroups_per_dimension: 65535,
                        max_immediate_size: 0,
                        max_non_sampler_bindings: 1_000_000,
                        max_task_workgroup_total_count: 0,
                        max_task_workgroups_per_dimension: 0,
                        max_mesh_workgroup_total_count: 0,
                        max_mesh_workgroups_per_dimension: 0,
                        max_task_invocations_per_workgroup: 0,
                        max_task_invocations_per_dimension: 0,
                        max_mesh_invocations_per_workgroup: 0,
                        max_mesh_invocations_per_dimension: 0,
                        max_task_payload_size: 0,
                        max_mesh_output_vertices: 0,
                        max_mesh_output_primitives: 0,
                        max_mesh_output_layers: 0,
                        max_mesh_multiview_view_count: 0,
                        max_blas_primitive_count: 0,
                        max_blas_geometry_count: 0,
                        max_tlas_instance_count: 0,
                        max_acceleration_structures_per_shader_stage: 0,
                        max_buffers_and_acceleration_structures_per_shader_stage: 28,
                        max_multiview_view_count: 0,
                        max_ray_dispatch_count: 0,
                        max_ray_recursion_depth: 0,
                        },
                        constrained_limits: None,
                        dx12_shader_compiler: bevy::render::settings::Dx12Compiler::Fxc,
                        gles3_minor_version: bevy::render::settings::Gles3MinorVersion::Automatic,
                        memory_hints: bevy::render::settings::MemoryHints::Performance,
                        instance_memory_budget_thresholds: Default::default(),
                        force_fallback_adapter: false,
                        adapter_name: None,
                    }),
                ),
                synchronous_pipeline_compilation: false,
                debug_flags: bevy::render::RenderDebugFlags::empty(),
            },
            bevy::image::ImagePlugin {
                default_sampler: bevy::image::ImageSamplerDescriptor {
                    label: None,
                    address_mode_u: bevy::image::ImageAddressMode::ClampToEdge,
                    address_mode_v: bevy::image::ImageAddressMode::ClampToEdge,
                    address_mode_w: bevy::image::ImageAddressMode::ClampToEdge,
                    mag_filter: bevy::image::ImageFilterMode::Linear,
                    min_filter: bevy::image::ImageFilterMode::Linear,
                    mipmap_filter: bevy::image::ImageFilterMode::Linear,
                    lod_min_clamp: 0.0,
                    lod_max_clamp: 32.0,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                },
            },
            bevy::mesh::MeshPlugin,
            bevy::camera::CameraPlugin,
            bevy::core_pipeline::CorePipelinePlugin,
            bevy::sprite::SpritePlugin,
            bevy::sprite_render::SpriteRenderPlugin,
            bevy::text::TextPlugin,
        ))
        .insert_resource(bevy::camera::ClearColor(bevy::color::Color::srgb(
            0.035, 0.035, 0.035,
        )))
        .insert_resource(bevy::time::Time::<bevy::time::Virtual>::from_max_delta(
            std::time::Duration::from_millis(250),
        ))
        .insert_resource(bevy::winit::WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::reactive_low_power(
                std::time::Duration::from_secs(60),
            ),
        })
        .add_systems(
            bevy::app::Update,
            move |window: bevy::ecs::system::Single<
                &bevy::window::Window,
                bevy::ecs::query::With<bevy::window::PrimaryWindow>,
            >,
             keys: bevy::ecs::system::Res<
                bevy::input::ButtonInput<bevy::input::keyboard::KeyCode>,
            >,
             mouse: bevy::ecs::system::Res<
                bevy::input::ButtonInput<bevy::input::mouse::MouseButton>,
            >,
             time: bevy::ecs::system::Res<bevy::time::Time>,
             kb: bevy::ecs::message::MessageReader<bevy::input::keyboard::KeyboardInput>,
             wheel: bevy::ecs::message::MessageReader<bevy::input::mouse::MouseWheel>,
             images: bevy::ecs::system::ResMut<bevy::asset::Assets<bevy::image::Image>>,
             commands: bevy::ecs::system::Commands,
             sprites: bevy::ecs::system::Query<
                (
                    bevy::ecs::change_detection::Mut<bevy::sprite::Sprite>,
                    bevy::ecs::change_detection::Mut<bevy::transform::components::Transform>,
                    bevy::ecs::change_detection::Mut<bevy::camera::visibility::Visibility>,
                ),
                bevy::ecs::query::Without<bevy::sprite::Text2d>,
            >,
             texts: bevy::ecs::system::Query<
                (
                    bevy::ecs::change_detection::Mut<bevy::sprite::Text2d>,
                    bevy::ecs::change_detection::Mut<bevy::text::TextFont>,
                    bevy::ecs::change_detection::Mut<bevy::text::TextColor>,
                    bevy::ecs::change_detection::Mut<bevy::sprite::Anchor>,
                    bevy::ecs::change_detection::Mut<bevy::transform::components::Transform>,
                    bevy::ecs::change_detection::Mut<bevy::camera::visibility::Visibility>,
                ),
                bevy::ecs::query::Without<bevy::sprite::Sprite>,
            >,
             winit: bevy::ecs::system::ResMut<bevy::winit::WinitSettings>,
             ma: bevy::ecs::system::Local<(
                u8,
                bool,
                bool,
                bool,
                f32,
                bool,
                usize,
                u8,
            )>,
             fo: bevy::ecs::system::Local<(
                String,
                Option<(usize, bevy::math::Vec2)>,
                u8,
                u8,
                std::path::PathBuf,
                Vec<(String, bool)>,
                usize,
                bool,
                f32,
                Vec<u8>,
            )>,
             st: bevy::ecs::system::Local<(
                Vec<(
                    String,
                    Option<(usize, bevy::math::Vec2)>,
                    u8,
                    u8,
                    u64,
                    [String; 2],
                )>,
                usize,
                Vec<(usize, Vec<u8>)>,
                (
                    std::path::PathBuf,
                    Vec<(String, bool, u64, bool)>,
                    usize,
                    usize,
                    bool,
                    Option<usize>,
                    Option<(std::sync::mpsc::Receiver<(usize, std::path::PathBuf, u64, bool)>, std::sync::Arc<Walk>, bool)>,
                    std::collections::HashMap<std::path::PathBuf, u64>,
                    bool,
                ),
            )>,
             pt: bevy::ecs::system::Local<(
                Option<usize>,
                Option<usize>,
                Option<(usize, f32)>,
                Vec<[f32; 4]>,
                [f32; 3],
                Vec<(bevy::math::Rect, usize, f32, f32, bool, f32)>,
                Option<(
                    std::sync::Arc<[std::sync::atomic::AtomicU32]>,
                    std::sync::Arc<std::sync::atomic::AtomicU32>,
                    std::sync::Arc<std::sync::atomic::AtomicBool>,
                    std::thread::Thread,
                    u32,
                    std::sync::mpsc::Sender<u8>, )>, (bevy::math::Vec2, f32, bool, bevy::math::Vec2, Option<bevy::math::Vec2>), )>,
             ic: bevy::ecs::system::Local<Vec<bevy::asset::Handle<bevy::image::Image>>>| {
                Some((kb, wheel, images, commands, sprites, texts, winit, ma, fo, st, pt, ic))
                    .iter_mut()
                    .for_each(|(kb, wheel, images, commands, sprites, texts, winit, ma, fo, st, pt, ic)| {
                if ic.is_empty() {
                    commands.spawn((bevy::camera::Camera2d, bevy::render::view::Msaa::Off));
                    ic.extend(
                        [
                            b"\
                              ....39dffffffffffffd93....\
                              ...9ffffffffffffffffff9...\
                              ..bffffffffffffffffffffb..\
                              .9ffffffffffffffffffffff9.\
                              3ffffffffffffffffffffffff3\
                              9ffffffffffffffffffffffff9\
                              dffffffffffffffffffffffffd\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              ffffffffffffffffffffffffff\
                              dffffffffffffffffffffffffd\
                              9ffffffffffffffffffffffff9\
                              3ffffffffffffffffffffffff3\
                              .9ffffffffffffffffffffff9.\
                              ..bffffffffffffffffffffb..\
                              ...9ffffffffffffffffff9...\
                              ....39dffffffffffffd93...."
                                .as_slice(),
                            b"\
                              ....39dffffffffffffd93....\
                              ...9d73............37d9...\
                              ..b9..................9b..\
                              .99....................99.\
                              3d......................d3\
                              97......................79\
                              d3......................3d\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              f........................f\
                              d3......................3d\
                              97......................79\
                              3d......................d3\
                              .99....................99.\
                              ..b9..................9b..\
                              ...9d73............37d9...\
                              ....39dffffffffffffd93...."
                                .as_slice(),
                            b"\
                              ffffffffffffffffffffffffff"
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........27abba72.........\
                              .......3bffffffffb3.......\
                              ......8fffda889dfff8......\
                              .....affc4......4cffa.....\
                              ....8ff8..........8ff8....\
                              ...3ff8............8ff3...\
                              ...bfc..............bfb...\
                              ..2ff4..............3ff2..\
                              ..7fd........2664....cf7..\
                              ..afa.......8ffffc2..9fa..\
                              ..bf8......8ffffffd1.7fb..\
                              ..bf8.....2ffffffff7.7fb..\
                              ..af9.....6ffffffffb.9fa..\
                              ..7fd.....6ffffffffc.cf7..\
                              ..2ff4....4ffffffff93ff2..\
                              ...bfc.....cfffffff3bfb...\
                              ...3ff8....2dfffff67ff3...\
                              ....8ff8....17bc937ff8....\
                              .....affb3......3bffa.....\
                              ......8fffc9779cfff8......\
                              .......3bffffffffb3.......\
                              .........27abba72.........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...........1771...........\
                              ..........3effe3..........\
                              .........4efccfe4.........\
                              ........6ffa22aff6........\
                              .......8ff822228ff8.......\
                              .....1afe62222226efa1.....\
                              ....1cfe5222222225efc1....\
                              ...3dfd322222222223dfd3...\
                              ..2efd32222222222223dfe2..\
                              ..2fff62222222222226fff2..\
                              ...3bfa222222222222afb3...\
                              ....7f92222222222229f7....\
                              ....7f92226777762229f7....\
                              ....7f9227ffffff7229f7....\
                              ....7f9228fdaadf8229f7....\
                              ....7f9228f9229f8229f7....\
                              ....7f9228f9229f8229f7....\
                              ....7fb55afb55bfa55bf7....\
                              ....3ffffffffffffffff3....\
                              .....5bcbbcbbbbcbbcb5.....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........231.............\
                              ........2bfffa1...........\
                              .......1efedffc...........\
                              .......8fc1.2ef5..........\
                              ..4eeeeff6...8ffeeee4.....\
                              ..7ffffff4...4ffffff7.....\
                              ..7f91111.........8f7.....\
                              ..7f8.............8f7.....\
                              ..7f8.............9ffa1...\
                              ..7fb1............4cffc...\
                              ..6ffe3.............2ef5..\
                              ...6cfb..............8f9..\
                              ....2ff..............8f9..\
                              ....4fe.............2ef5..\
                              ..2cff8...........4cffc...\
                              ..7ff9............9ffa1...\
                              ..7f9.............8f7.....\
                              ..7f8.............8f7.....\
                              ..7fffffffffffffffff7.....\
                              ..4fffffffffffffffff4.....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........33333333.........\
                              ........8ffffffff8........\
                              ........4dfeccefd4........\
                              .........3fc..cf3.........\
                              .........3fc..cf3.........\
                              .........3fc..cf3.........\
                              .........3fc..cf3.........\
                              .........3fc..cf3.........\
                              .........9fa..af9.........\
                              ........4ff2..2ff4........\
                              ........df7....7fd........\
                              .......8fc......cf8.......\
                              ......3ffa888888aff3......\
                              ......cffffffffffffc......\
                              .....7fd5555555555df7.....\
                              ....2ef4..........4fe2....\
                              ....bfa............afb....\
                              ...2ff1............1ff2...\
                              ....efcbbbbbbbbbbbbcfe....\
                              ....8ffffffffffffffff8....\
                              .....3444444444444443.....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ............22............\
                              ...........4ee4...........\
                              ..........5ffff5..........\
                              .........3ef99fe3.........\
                              .........cfa..afc.........\
                              ........4fe1331ef4........\
                              ........af9cffc9fa........\
                              ........efbfbbfbff........\
                              .......3fecf1.fcef3.......\
                              .......5fbaf98fabf5.......\
                              .......6fa2effe29f6.......\
                              .......af9.1661.9fb.......\
                              ......5ff9......9ff6......\
                              .....2eff9......9ffe2.....\
                              .....bfefa333333afdfc.....\
                              ....7fd7ffffffffff7df7....\
                              ....bf7cfeccccccefc6fc....\
                              ....bfffd3......3dfffc....\
                              ....bffa1.bc1.cc.1affc....\
                              ....af7...df87fd...7fa....\
                              .....2....4ffff5....2.....\
                              ...........9ffa...........\
                              ...........1de2...........\
                              ............11............\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....5999995....5999995....\
                              ...5fffffff5..5fffffff5...\
                              ...9fa6669fa..9fa6669fa...\
                              ...9f6...5fa..9f6...5fa...\
                              ...9f6...5fa..9fb888bfa...\
                              ...9f6...5fa..3effffff4...\
                              ...9f6...5fa...2677762....\
                              ...9f6...5fa..............\
                              ...8fdbbbdf9..............\
                              ...2dfffffe2...7ccccc8....\
                              ....1444441...6fffffff7...\
                              ..............9f83337fa...\
                              ..............9f6...5fa...\
                              ....7ccccc8...9f6...5fa...\
                              ...6fffffff7..9f6...5fa...\
                              ...9f83337fa..9f6...5fa...\
                              ...9f6...5fa..9f6...5fa...\
                              ...9f95558fa..9f95558fa...\
                              ...5fffffff6..5fffffff6...\
                              ....5aaaaa6....5aaaaa6....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ............77............\
                              ...........3ff3...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ....32222224ff52222223....\
                              ...7ffffffffffffffffff7...\
                              ...7ffffffffffffffffff7...\
                              ....34444445ff74444443....\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              ...........3ff3...........\
                              ............77............\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ............22............\
                              ..........17ee81..........\
                              ........17effffe71........\
                              .......6dffa32affe6.......\
                              .....5dffb3....3bffd6.....\
                              ...5dffc4........4bffd5...\
                              ..8ffe4............4eff8..\
                              ..4dffb3..........3bffd4..\
                              ....6dffa3......2affe6....\
                              .....17effa2..29ffe71.....\
                              ..392..17eff99ffe81..293..\
                              ..9fe81..18effe81..18ef9..\
                              ..19ffe81..19a2..18eff91..\
                              ....29ffe71....17eff92....\
                              ...3..2affe71.6effa2..3...\
                              ..8fb3..3affddffb3..3bf8..\
                              ..4effa2..3bffb4..2affe4..\
                              ...17eff92..34..29ffe71...\
                              .....18eff91..18ffe81.....\
                              .......18efe88eff81.......\
                              .........19ffff92.........\
                              ...........2992...........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....289999999999999982....\
                              ...2effffffffffffffffe3...\
                              ...afb66666666666666afb...\
                              ...cf3..............2fd...\
                              ...cf3.19a3.........2fd...\
                              ...cf3.9ffe.........2fd...\
                              ...cf3.afff1........2fd...\
                              ...cf3.3ef8.........2fd...\
                              ...cf3...22.........2fd...\
                              ...cf3..1cf5........2fd...\
                              ...cf3.1cfff5..5d7..2fd...\
                              ...cf31cfc8ff55fff7.2fd...\
                              ...cf4cfc1.7ffffaff72fd...\
                              ...cfefc1...7ff8.5ff9fd...\
                              ...cffc1.....56...5fffd...\
                              ...aff6555555555555affb...\
                              ...3efffffffffffffffff7...\
                              ....29aaaaaaaaaaaaaa93....\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........55.55.55.........\
                              .........ee.ee.ee.........\
                              .........de.de.de.........\
                              .........de.de.de.........\
                              .........de.de.de.........\
                              .......19ffcffcff91.......\
                              .......9ffffffffffa.......\
                              ..5edddff53333335ffddde5..\
                              ..5eeeeff3.566512ffeeee5..\
                              .......cf35ffff62fd.......\
                              ..5edddff36ffff72ffddde5..\
                              ..5eeeeff36ffff72ffeeee5..\
                              .......cf35ffff62fd.......\
                              ..5edddff31677612ffddde5..\
                              ..5eeeeff52222224ffeeee5..\
                              .......9ffffffffffa.......\
                              .......1affdffdffa1.......\
                              .........de.de.de.........\
                              .........de.de.de.........\
                              .........de.de.de.........\
                              .........ee.ee.ee.........\
                              .........55.55.55.........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....3acccccccccccccca4....\
                              ...4ffffffffffffffffff5...\
                              ...cf9333333333333338fd...\
                              ...ff................ef1..\
                              ...ff................ef1..\
                              ...ff.2b5............ef1..\
                              ...ff.5ff8...........ef1..\
                              ...ff..6ffb1.........ef1..\
                              ...ff...3efd2........ef1..\
                              ...ff....9ff4........ef1..\
                              ...ff..1bff6.........ef1..\
                              ...ff.2dfd3.16666661.ef1..\
                              ...ff.5fb1..9ffffff9.ef1..\
                              ...ff..3....39999993.ef1..\
                              ...ff................ef1..\
                              ...cf8222222222222227fd...\
                              ...4ffffffffffffffffff5...\
                              ....4bddddddddddddddb4....\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....3acccccccccccccca4....\
                              ...4ffffffffffffffffff5...\
                              ...cf9333333333333338fd...\
                              ...ff................ef1..\
                              ...ff5555555555555555ef1..\
                              ..7ffffffffffffffffffff8..\
                              ..2ff8888fd8888888888ff3..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...ff...2fa..........ef1..\
                              ...cf8224fb2222222227fd...\
                              ...4ffffffffffffffffff5...\
                              ....4bdddfedddddddddb4....\
                              .........31...............\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .......1342...............\
                              .....2affffc5.............\
                              ....4effcceff8............\
                              ...2efa2...6ff7...........\
                              ...afa......5fe1..........\
                              ..1ff2.......bf6..........\
                              ..3fc........7f9..........\
                              ..4fc........6f9..........\
                              ..2fe........9f7..........\
                              ...cf6.......331..........\
                              ...5ff5......699999994....\
                              ....8ffb76936fffffffff3...\
                              .....7effff39fa66666bf7...\
                              ......1699719f6.....8f7...\
                              ............9f6.....8f7...\
                              ............9f6.....8f7...\
                              ............9f6.....8f7...\
                              ............9f6.....8f7...\
                              ............9fb88888cf6...\
                              ............4ffffffffe2...\
                              .............377777762....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ............67............\
                              ..........5dffd5..........\
                              ........3bffccffb3........\
                              ......2affd6..5dffa2......\
                              ....18efe71....17eff81....\
                              ...6eff91........19ffe6...\
                              ...cffa1..........1affc...\
                              ...cfffe6........6efffc...\
                              ...cf5bffd5....4cffb5fc...\
                              ...cf3.4cffb33bffc4.3fc...\
                              ...cf3...6dffffe6...3fc...\
                              ...cf3....18ff81....3fc...\
                              ...cf3......ef1.....3fc...\
                              ...cf3......ef1.....3fc...\
                              ...cf3......ef1.....3fc...\
                              ...cfb3.....ef1....2bfc...\
                              ...6eff91...ef1..19ffe6...\
                              ....18efe71.ef117eff81....\
                              ......2affd6ef6dffa2......\
                              ........3bffffffb3........\
                              ..........5dffd5..........\
                              ............67............\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ............1763..........\
                              .......272..9fffe5........\
                              ......7efd1.ef9df9........\
                              .....affdfddfc.7f8..1.....\
                              ....1ff51efff7.9fc49fa....\
                              .....9fa.362...4efffff7...\
                              .....4ff1.......19fa5fe2..\
                              ..598cf9...1441...1.1df7..\
                              ..efffe1..7effe7...3efd2..\
                              .3fd794..7ffbbff7..6fd1...\
                              .6f9....1ef4..4fe1.2fd....\
                              .7ffc7..4fb....bf4..ffe91.\
                              .19eff..4fb....bf4..7cff7.\
                              ....df2.1ef4..4fe1....9f6.\
                              ...1df6..7ffbbff7..497df3.\
                              ..2dfe3...7effe7..1efffe..\
                              ..7fd1.1...1441...9fc895..\
                              ..2ef5af91.......1ff4.....\
                              ...7fffffe4...263.af9.....\
                              ....af94cf9.7fffe15ff1....\
                              .....1..8f7.cfddfdffa.....\
                              ........9fd9fe.1dfe7......\
                              ........5efff9..272.......\
                              ..........3671............\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ........1.................\
                              .......5fa1...............\
                              .......8ffe6..............\
                              .......8ffffb2............\
                              .......8fb7fff7...........\
                              .......8fa.2bffc3.........\
                              .......8fa...6eff91.......\
                              .......8fa....1affd5......\
                              .......8fa......4dffa2....\
                              .......8fa.......18fff2...\
                              .......8fa........7fff2...\
                              .......8fa......4dffb2....\
                              .......8fa....19ffe6......\
                              .......8fa...5effa1.......\
                              .......8fa.2affd4.........\
                              .......8fa6eff81..........\
                              .......8ffffc3............\
                              .......8ffe7..............\
                              .......5fb2...............\
                              ........1.................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ............44............\
                              ...........3ff3...........\
                              ...........2ff4...........\
                              ...........2ff4...........\
                              .......62..2ff4..26.......\
                              ......9fc..2ff4..cf9......\
                              .....7ff7..2ff4..7ff7.....\
                              ....2ff8...2ff4...8ff2....\
                              ....9fc....2ff4....cf9....\
                              ....ef5....2ff4....5fe....\
                              ...3ff1....3ff4....1ff3...\
                              ...4fd......aa......df4...\
                              ...4fe..............ef4...\
                              ...2ff1............1ff2...\
                              ....ef5............5fe....\
                              ....9fb............bf9....\
                              ....2ef7..........7fe2....\
                              .....7ff8........8ff7.....\
                              ......9ffc51..15cff9......\
                              .......7ffffddffff7.......\
                              ........29effffe92........\
                              ...........2442...........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...3444443................\
                              ..7fffffffa...............\
                              ..8fdccccef9..............\
                              ..8f822225ef94444443......\
                              ..8f8222225fffffffff7.....\
                              ..8f82222225ccccccdf8.....\
                              ..8f822354444444449fa32...\
                              ..8f82bffffffffffffffff6..\
                              ..8f89ffdddddddddddddefc..\
                              ..8f8ef63333333333333af9..\
                              ..8fdfc33333333333333ef3..\
                              ..8fff833333333333337fc...\
                              ..8ffe43333333333333cf6...\
                              ..8ffa33333333333334ff1...\
                              ..8ff533333333333339fa....\
                              ..8ffbbbbbbbbbbbbbbff4....\
                              ..9fffffffffffffffff8.....\
                              ..166666666666666663......\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....23333.................\
                              ..1bfffffe7...............\
                              ..7fedddeffa..............\
                              ..9f822227efb533333332....\
                              ..9f7222226effffffffffb1..\
                              ..9f722222249cdddddddef7..\
                              ..9f722222222222222228f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f722222222222222227f9..\
                              ..9f822222222222222228f9..\
                              ..8fdaaaaaaaaaaaaaaaadf8..\
                              ..2dffffffffffffffffffd2..\
                              ...16666666666666666661...\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ......2666666652..........\
                              .....4fffffffffe5.........\
                              .....bfcaaaaaaffe5........\
                              .....cf5222222de8e5.......\
                              .....cf4222222de68e5......\
                              .....cf4222222de668e5.....\
                              .....cf4222222dfeeefe2....\
                              .....cf4222222aeeeeff5....\
                              .....cf422222222222af7....\
                              .....cf4222222222229f7....\
                              .....cf4222222222229f7....\
                              .....cf43aaaaaaaa829f7....\
                              .....cf44ffffffffc29f7....\
                              .....cf4223333333229f7....\
                              .....cf4222222222229f7....\
                              .....cf43dddddc22229f7....\
                              .....cf43dddddc22229f7....\
                              .....cf4222222222229f7....\
                              .....cf522222222222af7....\
                              .....bfcaaaaaaaaaaadf6....\
                              .....4ffffffffffffffd1....\
                              ......266666666666651.....\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ............11............\
                              ...........2ee2...........\
                              ..........2dffd2..........\
                              .........2dffffd2.........\
                              ........2dfeffefd2........\
                              .......2dfe4ff4dfd2.......\
                              ......2dfd21ff22dfd2......\
                              .....2dfd2.1ff2.2dfd3.....\
                              .....7fd2..1ff2..2df8.....\
                              .....183...1ff2...381.....\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff2...........\
                              ...........1ff1...........\
                              ............66............\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...18cccccccccccccccc91...\
                              ...cffffffffffffffffffd1..\
                              ..4fd4333333333333333df5..\
                              ..6f9.24.15..51.42.338f7..\
                              ..6f9.af28f55f82fb.ee8f7..\
                              ..6f9.37.281182.74.668f7..\
                              ..6f9.37..........74.8f7..\
                              ..6f9.af2........2fb.8f7..\
                              ..6f9.24.13333331.42.8f7..\
                              ..6f9....affffffa....8f7..\
                              ..6f9....49999994....8f7..\
                              ..4fd3222222222222222cf5..\
                              ...cffffffffffffffffffd1..\
                              ...19dddddddddddddddda2...\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .................33433....\
                              ................6fffff3...\
                              ................16bf96....\
                              ..................8f5.....\
                              ..................8f5.....\
                              ..................8f5.....\
                              ..................8f5.....\
                              ..6eeeeee3eeeeec..8f5.....\
                              ..7fffffe4effffd..8f5.....\
                              ...11111...1111...8f5.....\
                              ..................8f5.....\
                              ..................8f5.....\
                              ..................8f5.....\
                              ................16af96....\
                              ................6fffff3...\
                              .................44443....\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..................391.....\
                              ..................9f6.....\
                              ..................8f7.....\
                              ..................8f7.....\
                              ...........3......8f7.....\
                              .........1bf5.....8f7.....\
                              ........1bfe2.....8f7.....\
                              .......1cfd2......8f7.....\
                              ......1cfd2.......8f7.....\
                              ......cffdccccccccef7.....\
                              .....2ffffffffffffff4.....\
                              ......5ffb3333333332......\
                              .......4ef9...............\
                              ........4efa..............\
                              .........3ef6.............\
                              ..........391.............\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ....1551..................\
                              ...4effe4.................\
                              ..1ef99fe1................\
                              ..5f9..9f56eeeeeeeeeeee6..\
                              ..5f9..9f57ffffffffffff7..\
                              ..1ef99fe1.111111111111...\
                              ...4eaae4.................\
                              ....1881..................\
                              .....78...................\
                              .....78...................\
                              .....7866631bbbbbbbbbbc4..\
                              .....7999952fffffffffff8..\
                              .....78111..24444444444...\
                              .....78...................\
                              .....78...................\
                              .....78...................\
                              .....78...................\
                              .....784442.888888893.....\
                              .....7999963ffffffff9.....\
                              .....122221.577777761.....\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              fb3.......................\
                              fffb3.....................\
                              fffffa3...................\
                              fffffffa3.................\
                              fffffffffa2...............\
                              fffffffffffa2.............\
                              fffffffffffff92...........\
                              fffffffffffffff92.........\
                              fffffffffffffffff92.......\
                              fffffffffffffffffff92.....\
                              ffffffffffffffffffffe81...\
                              ffffffffffffffffffffffe81.\
                              ffffffffffffffffffffffffe8\
                              fffffffffffffffffffffffff9\
                              fffffffffffffffffffffff92.\
                              fffffffffffffffffffff92...\
                              fffffffffffffffffffa2.....\
                              fffffffffffffffffa2.......\
                              fffffffffffffffa3.........\
                              fffffffffffffa3...........\
                              fffffffffffb3.............\
                              fffffffffb3...............\
                              fffffffb4.................\
                              fffffb4...................\
                              fffc4.....................\
                              fc4......................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ...........1221...........\
                              ......26adffffffda62......\
                              ....2affffeddddeffffa2....\
                              ...1efea6322222236aefe1...\
                              ...8fc32222222222223cf8...\
                              ...9fc32222222222223cf9...\
                              ...8ffea6322222236aeff9...\
                              ...8feffffeddddeffffdf9...\
                              ...8f847beffffffeb747f9...\
                              ...8f8222223443222227f9...\
                              ...7a7222222222222226a8...\
                              ...78622222222222222688...\
                              ...8a8643222222223468a9...\
                              ...8f9788766666678878f9...\
                              ...8f8235677887765327f9...\
                              ...8f8222222222222227f9...\
                              ...9f8222222222222227f9...\
                              ...8fc32222222222223cf8...\
                              ...1efe964222222469efe1...\
                              ....2affffeddddeffffa2....\
                              ......26bdffffffdb62......\
                              ...........2322...........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...............251........\
                              ..............5ffe3.......\
                              .............5ffefe3......\
                              ............5ffa2bfe3.....\
                              ...........5fffe52bfe1....\
                              ..........5ff97fe52ef5....\
                              .........5ff9227feaff2....\
                              ........5ff922227fff5.....\
                              .......5ff9222229ff5......\
                              ......5ff9222229ff5.......\
                              .....5ff9222229ff5........\
                              ....2ff9222229ff5.........\
                              ....afb222229ff5..........\
                              ....ef522229ff5...........\
                              ...3fe22229ff5............\
                              ...6fb225bff5.............\
                              ...8fcbefff5..............\
                              ...7ffffe92...............\
                              ....7851..................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ......2666666652..........\
                              .....4fffffffffe5.........\
                              .....bfcaaaaaaffe5........\
                              .....cf5222222de8e5.......\
                              .....cf4222222de68e5......\
                              .....cf4222222de668e5.....\
                              .....cf4222222dfeeefe2....\
                              .....cf4222222aeeeeff5....\
                              .....cf422222222222af7....\
                              .....cf4222222222229f7....\
                              .....cf425ec22222229f7....\
                              .....cf42aff52222229f7....\
                              .....cf424ca22222229f7....\
                              .....cf4222462222229f7....\
                              .....cf4224efa3b7229f7....\
                              .....cf424efdfeff429f7....\
                              .....cf44ef62bf9fd29f7....\
                              .....cf6ef6222327fa9f7....\
                              .....cf6d62222222aaaf7....\
                              .....bfcaaaaaaaaaaadf6....\
                              .....4ffffffffffffffd1....\
                              ......266666666666651.....\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ............22............\
                              ...........1ee1...........\
                              ...........7ff8...........\
                              ...........effe1..........\
                              ..........6fedf7..........\
                              ..........df76fd..........\
                              .........5fe11ef6.........\
                              .........cf8..8fc.........\
                              ........4ff2..1ef4........\
                              ........bf9....9fb........\
                              .......3ff2....2ff3.......\
                              .......afc333333bfa.......\
                              ......2ffffffffffff2......\
                              ......9feccccccccef9......\
                              .....1ef5........4fe1.....\
                              .....8fd..........cf8.....\
                              ....1ef6..........5fe1....\
                              ....7fd............df7....\
                              ....af7............7fa....\
                              ....14..............41....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....389888888983..........\
                              ...3ffffffffffff3.........\
                              ...9fb77777777bf9.........\
                              ...9f6........6f9.........\
                              ...9f6........6f9.........\
                              ...9f6....13337fa3332.....\
                              ...9f6...6fffffffffffb1...\
                              ...9f6..1ffdddddddddef7...\
                              ...9f6..4fd2222222228f9...\
                              ...9f6..4fc2222222227f9...\
                              ...9f6..4fc2222222227f9...\
                              ...9fb779fc2222222227f9...\
                              ...3ffffffc2222222227f9...\
                              ....3898afc2222222227f9...\
                              ........4fc2222222227f9...\
                              ........4fc2222222227f9...\
                              ........4fc2222222227f9...\
                              ........2fe888888888cf9...\
                              .........bffffffffffff3...\
                              .........1798888888983....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ....16..............61....\
                              ....9fa............af9....\
                              ....3ef9..........9fe3....\
                              .....3ef8........8fe3.....\
                              ......4ff7......7ff4......\
                              .......5ff6....6ff5.......\
                              ........6ff5..5ff6........\
                              .........7ff44ff7.........\
                              ..........8feef8..........\
                              ...........affa...........\
                              ..........2effe2..........\
                              .........2dfbbfd2.........\
                              ........1cfc11cfc1........\
                              ....1786cfc1..1cfc6871....\
                              ...4effffd1....1dffffe4...\
                              ..1ef95afb......bfb59fe1..\
                              ..4fa...df2....2fd...af4..\
                              ..5f8...bf3....3fc...8f5..\
                              ..2fe3.5fe......ef5.3ef2..\
                              ...9ffeff6......6ffeff9...\
                              ....7dfc5........5cfd7....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........9cbbbbc9.........\
                              ........7ffffffff7........\
                              .....266bfb9999bfb662.....\
                              ....6fffffa7777afffff6....\
                              ....efbadfcbbbbcfdabfe....\
                              ....ff327ffffffff722ff1...\
                              ....ff2227a9999a7222ef1...\
                              ....ff22222222222222ef1...\
                              ....ff22222222222222ef1...\
                              ....ff22222222222222ef1...\
                              ....ff228ffffffff822ef1...\
                              ....ff225aaaaaaaa522ef1...\
                              ....ff22222222222222ef1...\
                              ....ff22222222222222ef1...\
                              ....ff225a9999932222ef1...\
                              ....ff229ffffff42222ef1...\
                              ....ff22233333322222ef1...\
                              ....ff22222222222222ef1...\
                              ....ff32222222222222ff1...\
                              ....efbaaaaaaaaaaaabfe....\
                              ....6ffffffffffffffff6....\
                              .....2666666666666662.....\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........25555552.........\
                              .........cffffffd.........\
                              .........df8888ee.........\
                              ....88899ef8888ef99888....\
                              ...3ffffffffffffffffff3...\
                              ....59fd8888888888df95....\
                              .....4fc2222222222cf4.....\
                              .....3fd2242332422df3.....\
                              .....1fe24f6bc6f42ef1.....\
                              ......ff23f8bd7f42ff......\
                              ......ef32f8bd7f33fe......\
                              ......df42f9bd8f34fd......\
                              ......cf52e9bd9f25fc......\
                              ......af62eabd9e26fa......\
                              ......9f72dbbdae27f9......\
                              ......8f82a8898a28f8......\
                              ......7fa22222222af7......\
                              ......5fd88888888df5......\
                              ......1effffffffffe1......\
                              .......289888888982.......\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...2acbbbbbbbbcc7.........\
                              ..2efffffffffffffa...781..\
                              ..8fc55555555557ff13cff8..\
                              ..9f822373222222dfbffff9..\
                              ..9f722bfe622222cfffbaf9..\
                              ..9f722dfffc5222cfe97af9..\
                              ..9f722dfdfff522df977af9..\
                              ..9f722dfdfff522df977af9..\
                              ..9f722dfffc5222cfe97af9..\
                              ..9f722bfe622222cfffbaf9..\
                              ..9f822373222222dfbffff9..\
                              ..8fc55555555557ff13cff8..\
                              ..2efffffffffffffa...781..\
                              ...2acbbbbbbbbcc7.........\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                        ]
                        .map(|art: &[u8]| {
                            images.add(bevy::image::Image::new(
                                bevy::render::render_resource::Extent3d {
                                    width: 26,
                                    height: art.len() as u32 / 26,
                                    depth_or_array_layers: 1,
                                },
                                bevy::render::render_resource::TextureDimension::D2,
                                art.iter()
                                    .flat_map(|glyph| {
                                        [
                                            255,
                                            255,
                                            255,
                                            (*glyph as char).to_digit(16).map_or(0, |digit| digit as u8 * 17),
                                        ]
                                    })
                                    .collect(),
                                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                                bevy::asset::RenderAssetUsages::RENDER_WORLD,
                            ))
                        }),
                    );
                    Some(ic[2].clone()).into_iter().for_each(|screen| ic.push(screen));
                    st.0 = redb::ReadableDatabase::begin_read(&db)
                        .unwrap()
                        .open_table(redb::TableDefinition::<&str, &[u8]>::new("fs"))
                        .into_iter()
                        .flat_map(|table| {
                            table
                                .range("/projects/"..="/projects/\u{10ffff}")
                                .unwrap()
                                .flatten()
                                .filter(|(key, _)| key.value().ends_with("/meta"))
                                .map(|(key, value)| {
                                    (
                                        String::from_utf8_lossy(&value.value()[2..]).into_owned(),
                                        table
                                            .get(format!("{}icon.png", &key.value()[..27]).as_str())
                                            .unwrap()
                                            .map(|png| png.value().to_vec()),
                                        value.value()[0],
                                        value.value()[1],
                                        u64::from_str_radix(&key.value()[10..26], 16).unwrap(),
                                        table
                                            .get(format!("{}line", &key.value()[..27]).as_str())
                                            .unwrap()
                                            .map_or(String::new(), |text| String::from_utf8_lossy(text.value()).into_owned()),
                                        table
                                            .get(format!("{}prompt", &key.value()[..27]).as_str())
                                            .unwrap()
                                            .map_or(String::from("> "), |text| String::from_utf8_lossy(text.value()).into_owned()),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .enumerate()
                        .map(|(row, (name, png, kind, dim, id, line, prompt))| {
                            st.2.extend(png.map(|bytes| (row, bytes)));
                            (name, None, kind, dim, id, [line, prompt])
                        })
                        .collect();
                    commands.spawn_batch((0..272).map(|_| {
                        (
                            bevy::sprite::Sprite {
                                image: bevy::asset::Handle::Uuid(bevy::asset::AssetId::<bevy::image::Image>::DEFAULT_UUID, std::marker::PhantomData),
                                texture_atlas: None,
                                color: bevy::color::Color::WHITE,
                                flip_x: false,
                                flip_y: false,
                                custom_size: Some(bevy::math::Vec2::ONE),
                                rect: None,
                                image_mode: bevy::sprite::SpriteImageMode::Auto,
                                alpha_mode: bevy::sprite::SpriteAlphaMode::Blend,
                            },
                            bevy::transform::components::Transform::IDENTITY,
                            bevy::camera::visibility::Visibility::Hidden,
                        )
                    }));
                    commands.spawn_batch((0..128).map(|_| {
                        (
                            bevy::sprite::Text2d::new(""),
                            bevy::text::TextFont {
                                font: bevy::text::FontSource::Handle(bevy::asset::Handle::Uuid(bevy::asset::AssetId::<bevy::text::Font>::DEFAULT_UUID, std::marker::PhantomData)),
                                font_size: bevy::text::FontSize::Rem(1.0),
                                style: bevy::text::FontStyle::Normal,
                                weight: bevy::text::FontWeight::NORMAL,
                                width: bevy::text::FontWidth::NORMAL,
                                font_features: bevy::text::FontFeatures::builder().build(),
                                font_variations: bevy::text::FontVariations::builder().build(),
                                font_smoothing: bevy::text::FontSmoothing::AntiAliased,
                            },
                            bevy::text::TextColor::WHITE,
                            bevy::sprite::Anchor::CENTER_LEFT,
                            bevy::transform::components::Transform::IDENTITY,
                            bevy::camera::visibility::Visibility::Hidden,
                        )
                    }));
                    ma.5 = true;
                }
                pt.3.resize(38 + 2 * st.0.len(), [0.0; 4]);
                Some((
                    (
                        std::ops::DerefMut::deref_mut(ma),
                        std::ops::DerefMut::deref_mut(fo),
                        std::ops::DerefMut::deref_mut(st),
                        std::ops::DerefMut::deref_mut(pt),
                        std::ops::DerefMut::deref_mut(ic),
                    ),
                    window.width(),
                    window.height(),
                ))
                    .map(|(l, w, h)| (matches!(l.0.0, 3 | 5), l, w, h))
                    .map(|(tight, l, w, h)| (l, w, h, if tight { 4.0 } else { (w * 0.22).clamp(240.0, 304.0).min(w * 0.5) }))
                    .map(|(l, w, h, side)| (l, w, h, side, ((w - side - 68.0) / 396.0).clamp(1.0, 8.0) as usize))
                    .map(|(l, w, h, side, cols)| {
                        (
                            l,
                            w,
                            h,
                            side,
                            (side + 424.0).min((w - 48.0).max(side + 45.0)),
                            time.delta_secs(),
                            bevy::math::Rect::from_center_size(
                                bevy::math::Vec2::new(w / 2.0, h / 2.0),
                                bevy::math::Vec2::new(640.0, 484.0),
                            ),
                            bevy::math::Rect::from_center_size(
                                bevy::math::Vec2::new(w / 2.0, h / 2.0),
                                bevy::math::Vec2::new(560.0, 440.0),
                            ),
                            bevy::math::Rect::new(side + 40.0, 108.0, (w - 44.0).max(side + 41.0), (h - 44.0).max(109.0)),
                            (
                                cols,
                                (((h - 136.0) / 160.0).max(1.0) as usize).min(16 / cols),
                                (((h - 200.0) / 36.0).max(1.0) as usize).min(30),
                            ),
                            (
                                mouse.just_pressed(bevy::input::mouse::MouseButton::Left),
                                mouse.just_released(bevy::input::mouse::MouseButton::Left),
                                mouse.pressed(bevy::input::mouse::MouseButton::Left),
                            ),
                        )
                    })
                    .into_iter()
                    .for_each(
                        |(
                            (
                                (view, hud, modal, picker, esc, moving, ed, edit),
                                (name, icon, kind, dim, dir, entries, pscroll, dirty, wacc, pending),
                                (store, lscroll, jobs, (xdir, xlist, xscroll, xsel, xdirty, xhit, xjob, xcache, xmenu)),
                                (hover, focus, press, anim, fades, bt, machine, (kc, kh, kact, km, ptr)),
                                icons,
                            ),
                            w,
                            h,
                            side,
                            xr,
                            dt,
                            mrect,
                            prect,
                            cv,
                            (cols, rows, vis),
                            (mdn, mup, mhd),
                        )| {
                            *hud ^= keys.just_pressed(bevy::input::keyboard::KeyCode::F1);
                            *wacc += wheel
                                .read()
                                .map(|e| {
                                    if e.unit == bevy::input::mouse::MouseScrollUnit::Line {
                                        e.y
                                    } else {
                                        e.y / 40.0
                                    }
                                })
                                .sum::<f32>();
                            for e in kb.read() {
                                if e.state.is_pressed() && *view == 4 && !keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) {
                                    machine
                                        .iter()
                                        .filter(|_| store.get(*ed).is_some_and(|pr| pr.2 == 2))
                                        .for_each(|(_, _, _, thread, _, tx)| {
                                            std::iter::once(8_u8)
                                                .filter(|_| e.logical_key == bevy::input::keyboard::Key::Backspace)
                                                .chain(std::iter::once(10_u8).filter(|_| e.logical_key == bevy::input::keyboard::Key::Enter))
                                                .chain(
                                                    e.text
                                                        .iter()
                                                        .flat_map(|t| t.chars())
                                                        .filter(|c| c.is_ascii_graphic() || *c == ' ')
                                                        .map(|c| c as u8),
                                                )
                                                .for_each(|byte| {
                                                    tx.send(byte).ok();
                                                    thread.unpark();
                                                });
                                        });
                                }
if e.state.is_pressed() && *edit > 0 && *view == 3 && !*hud && !keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) {
                                    *moving = true;
                                    store.get_mut(*ed).into_iter().for_each(|pr| {
                                        for (id, key) in [(pr.4, if *edit == 11 { "line" } else { "prompt" })] {
                                        pr.5.get_mut(*edit as usize - 11).into_iter().for_each(|field| {
                                        if e.logical_key == bevy::input::keyboard::Key::Backspace {
                                            field.pop();
                                        }
                                        field.extend(
                                            e.text
                                                .iter()
                                                .flat_map(|t| t.chars())
                                                .filter(|c| c.is_ascii_graphic() || *c == ' ')
                                                .take(24_usize.saturating_sub(field.len())),
                                        );
                                        Some(db.begin_write().unwrap()).into_iter().for_each(|tx| {
                                            Some(tx.open_table(redb::TableDefinition::<&str, &[u8]>::new("fs")).unwrap())
                                                .iter_mut()
                                                .for_each(|table| {
                                                    table
                                                        .insert(format!("/projects/{id:016x}/{key}").as_str(), field.as_bytes())
                                                        .unwrap();
                                                });
                                            tx.commit().unwrap();
                                        });
                                        });
                                        }
                                    });
                                }
                                if e.state.is_pressed() && *modal && !*picker && !*hud && !keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) {
                                    *moving = true;
                                    if e.logical_key == bevy::input::keyboard::Key::Backspace {
                                        name.pop();
                                    }
                                    name.extend(
                                        e.text
                                            .iter()
                                            .flat_map(|t| t.chars())
                                            .filter(|c| c.is_ascii_graphic() || *c == ' ')
                                            .take(24 - name.len()),
                                    );
                                }
                            }
                            *moving |= !jobs.is_empty();
                            jobs.drain(..).for_each(|(target, bytes)| {
                                bevy::image::Image::from_buffer(
                                    &bytes,
                                    bevy::image::ImageType::Extension("png"),
                                    bevy::image::CompressedImageFormats::NONE,
                                    true,
                                    bevy::image::ImageSampler::Default,
                                    bevy::asset::RenderAssetUsages::RENDER_WORLD,
                                )
                                .map(|img| {
                                    match (
                                        img.size_f32(),
                                        if target == usize::MAX { icon.map_or(icons.len(), |c| c.0) } else { icons.len() },
                                    ) {
                                        (size, slot) => {
                                            if slot == icons.len() {
                                                icons.push(images.add(img));
                                            } else {
                                                icons[slot] = images.add(img);
                                            }
                                            if target == usize::MAX {
                                                *icon = Some((slot, size / size.max_element()));
                                                *pending = bytes;
                                            } else {
                                                store[target].1 = Some((slot, size / size.max_element()));
                                            }
                                        }
                                    }
                                })
                                .ok();
                            });
                            bt.clear();
                            Some((sprites.iter_mut(), texts.iter_mut()))
                                .iter_mut()
                                .for_each(|(sit, tit)| {
                                    Some((
                                        |r: bevy::math::Rect, z: f32, c: bevy::color::Color, i: usize| {
                                            sit.next().iter_mut().for_each(|(s, t, v)| {
                                                if s.color != c {
                                                    s.color = c;
                                                }
                                                if s.image != icons[i] {
                                                    s.image = icons[i].clone();
                                                }
                                                if s.custom_size != Some(r.size()) {
                                                    s.custom_size = Some(r.size());
                                                }
                                                Some(if i < 2 {
                                                    bevy::sprite::SpriteImageMode::Sliced(bevy::sprite::TextureSlicer {
                                                        border: bevy::sprite::BorderRect::all(9.0),
                                                        center_scale_mode: bevy::sprite::SliceScaleMode::Stretch,
                                                        sides_scale_mode: bevy::sprite::SliceScaleMode::Stretch,
                                                        max_corner_scale: 1.0,
                                                    })
                                                } else {
                                                    bevy::sprite::SpriteImageMode::Auto
                                                })
                                                .filter(|m| s.image_mode != *m)
                                                .into_iter()
                                                .for_each(|m| s.image_mode = m);
                                                Some(bevy::math::Vec3::new(r.center().x - w / 2.0, h / 2.0 - r.center().y, z))
                                                    .filter(|p| t.translation != *p)
                                                    .into_iter()
                                                    .for_each(|p| t.translation = p);
                                                bevy::ecs::change_detection::DetectChangesMut::set_if_neq(v, bevy::camera::visibility::Visibility::Inherited);
                                            });
                                        },
                                        |x: f32, y: f32, an: usize, label: &str, size: f32, z: f32, c: bevy::color::Color| {
                                            tit.next().iter_mut().for_each(|(tx, tf, tc, ta, t, v)| {
                                                if tx.0 != label {
                                                    tx.0.clear();
                                                    tx.0.push_str(label);
                                                }
                                                if tf.font_size != bevy::text::FontSize::Px(size) {
                                                    tf.font_size = bevy::text::FontSize::Px(size);
                                                }
                                                if tc.0 != c {
                                                    tc.0 = c;
                                                }
                                                Some([
                                                    bevy::sprite::Anchor::CENTER_LEFT,
                                                    bevy::sprite::Anchor::TOP_LEFT,
                                                    bevy::sprite::Anchor::CENTER,
                                                ][an])
                                                .filter(|a| **ta != *a)
                                                .into_iter()
                                                .for_each(|a| **ta = a);
                                                Some(bevy::math::Vec3::new(x - w / 2.0, h / 2.0 - y, z))
                                                    .filter(|p| t.translation != *p)
                                                    .into_iter()
                                                    .for_each(|p| t.translation = p);
                                                bevy::ecs::change_detection::DetectChangesMut::set_if_neq(v, bevy::camera::visibility::Visibility::Inherited);
                                            });
                                        },
                                        |v: f32, a: f32| bevy::color::Color::srgba(v, v, v, a),
                                        |x0: f32, y0: f32, x1: f32, y1: f32| bevy::math::Rect::new(x0, y0, x1, y1),
                                        |cx: f32, cy: f32, sx: f32, sy: f32| {
                                            bevy::math::Rect::from_center_size(
                                                bevy::math::Vec2::new(cx, cy),
                                                bevy::math::Vec2::new(sx, sy),
                                            )
                                        },
                                    ))
                                    .iter_mut()
                                    .for_each(|(spr, txt, gr, rc, cs)| {
                                        if !*hud && (*view < 4 || *view == 5) {
for dot in [bevy::math::Vec2::new(kc.x.floor(), kc.y.floor())].into_iter().filter(|_| *kact) {
    spr(rc(dot.x, dot.y, dot.x + 1.0, dot.y + 1.0), 9.9, gr(1.0, 1.0), 2);
}
                                          if *view != 3 && *view != 5 {
                                            spr(rc(24.0, 24.0, side - 4.0, (h - 24.0).max(48.0)), 1.0, gr(0.071, 1.0), 0);
                                            spr(rc(24.0, 24.0, side - 4.0, (h - 24.0).max(48.0)), 1.1, gr(0.133, 1.0), 1);
                                            spr(rc(40.0, 84.0, side - 20.0, 85.0), 3.0, gr(0.133, 1.0), 2);
                                            for (index, label) in [if *view == 2 { (6, "Laboratory") } else { (3, title) }] {
                                                spr(cs(54.0, 56.0, 18.0, 18.0), 6.0, gr(0.93, 1.0), index);
                                                txt(72.0, 46.0, 1, label, ((side - 88.0) / (0.6 * label.chars().count() as f32)).clamp(9.0, 15.0), 6.0, gr(0.93, 1.0));
                                            }
                                            if *view < 2 {
                                                for (k, (r, label)) in [
                                                    (rc(36.0, 100.0, side - 16.0, 152.0), "Home"),
                                                    (rc(36.0, 160.0, side - 16.0, 212.0), "Extension Registry"),
                                                ]
                                                .into_iter()
                                                .enumerate()
                                                {
                                                    bt.push((r, k, (*view as usize == k) as u8 as f32, 5.0, !*modal && !*picker, 1.0));
                                                    spr(
                                                        cs(64.0, r.center().y, 20.0, 20.0),
                                                        6.0,
                                                        gr(0.6 + 0.33 * anim[k][0].max(anim[k][3]), 1.0),
                                                        4 + k,
                                                    );
                                                    txt(84.0, r.center().y, 0, label, 15.0, 6.5, gr(0.6 + 0.33 * anim[k][0].max(anim[k][3]), 1.0));
                                                    spr(rc(40.0, r.min.y + 15.0, 43.0, r.max.y - 15.0), 6.0, gr(0.93, anim[k][3]), 2);
                                                }
                                            } else if *view == 2 {
                                                bt.push((rc(36.0, 100.0, side - 16.0, 152.0), 3, 0.0, 5.0, !*modal && !*picker, 1.0));
                                                spr(cs(64.0, 126.0, 18.0, 18.0), 6.0, gr(0.7 + 0.23 * anim[3][0], 1.0), 9);
                                                txt(84.0, 126.0, 0, "Create project", 15.0, 6.5, gr(0.7 + 0.23 * anim[3][0], 1.0));
                                            }
                                          }
                                            if *view > 0 {
                                                for a in [fades[0]] {
                                                    spr(rc(side + 20.0, 24.0, (w - 24.0).max(side + 60.0), (h - 24.0).max(48.0)), 1.0, gr(0.071, a), 0);
                                                    spr(rc(side + 20.0, 24.0, (w - 24.0).max(side + 60.0), (h - 24.0).max(48.0)), 1.1, gr(0.133, a), 1);
                                                    if *view == 1 {
                                                        for (cd, icon, title, caption, id) in [
                (rc(side + 44.0, 48.0, xr, 192.0), 6, "Laboratory", "OS, CLI and GUI projects", 2),
                (rc(side + 44.0, 208.0, xr, 352.0), 27, "Explorer", "Files and folders", 13),
            ] {
                                                            for hv in [ptr.is_some_and(|c| cd.contains(c) && !*modal && !*picker) as u8 as f32] {
                                                                spr(cd, 1.6, gr(0.102 + 0.012 * hv, a), 0);
                                                                spr(cd, 1.7, gr(0.157 + 0.06 * hv, a), 1);
                                                            }
                                                            txt(cd.min.x + 22.0, cd.max.y - 34.0, 0, caption, 10.0, 6.5, gr(0.42, a));
                                                            spr(rc(cd.min.x + 20.0, cd.min.y + 64.0, cd.max.x - 20.0, cd.min.y + 65.0), 1.8, gr(0.157, a), 2);
                                                            spr(cs(cd.min.x + 35.0, cd.min.y + 32.0, 26.0, 26.0), 6.0, gr(0.93, a), icon);
                                                            txt(cd.min.x + 62.0, cd.min.y + 32.0, 0, title, 18.0, 6.5, gr(0.93, a));
                                                            for lr in [rc(cd.max.x - 128.0, cd.max.y - 52.0, cd.max.x - 16.0, cd.max.y - 16.0)] {
                                                                bt.push((lr, id, 0.0, 5.0, !*modal && !*picker, a));
                                                                spr(cs(lr.min.x + 22.0, lr.center().y, 18.0, 18.0), 6.0, gr(0.93, a), 7);
                                                                txt(lr.min.x + 40.0, lr.center().y, 0, "Launch", 14.0, 6.5, gr(0.93, a));
                                                            }
                                                        }
                                                    } else if *view == 5 {
for (left, right, col) in [(side + 48.0, (w - 52.0).max(side + 49.0), (w - 52.0).max(side + 49.0) - 140.0)] {
    spr(cs(left + 14.0, 62.0, 28.0, 28.0), 6.0, gr(0.93, a), 27);
    txt(left + 40.0, 62.0, 0, "Explorer", 20.0, 6.5, gr(0.93, a));
    spr(rc(left, 98.0, right, 99.0), 1.8, gr(0.133, a), 2);
    for (whole, home) in [(xdir.as_path(), std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")))] {
        for (names, room, fit) in [(
            whole
                .components()
                .enumerate()
                .filter(|(i, c)| *i == 0 || !matches!(c, std::path::Component::RootDir))
                .map(|(_, c)| c.as_os_str().to_string_lossy().chars().take(28).collect::<String>())
                .collect::<Vec<_>>(),
            right - (left + 176.0) - 140.0,
            |n: &String| 38.0 + if n.is_empty() { 18.0 } else { 8.4 * n.chars().count() as f32 },
        )] {
            for names in [names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    if home.as_ref().is_some_and(|base| whole.ancestors().nth(names.len() - 1 - i) == Some(std::path::Path::new(base))) {
                        String::new()
                    } else {
                        n.clone()
                    }
                })
                .collect::<Vec<_>>()]
            {
                for k in [if names.len() <= 10 && names.iter().map(fit).sum::<f32>() <= room {
                    names.len()
                } else {
                    names
                        .iter()
                        .rev()
                        .map(fit)
                        .scan(56.0, |total, width| {
                            *total += width;
                            Some(*total)
                        })
                        .take_while(|total| *total <= room)
                        .count()
                        .clamp(1, 9)
                        .min(names.len())
                }] {
                    for shown in [std::iter::once("..")
                        .filter(|_| k < names.len())
                        .chain(names[names.len() - k..].iter().map(String::as_str))
                        .collect::<Vec<_>>()]
                    {
                        for segs in [shown
                            .iter()
                            .enumerate()
                            .scan(left + 176.0, |edge, (j, label)| {
                                [(if j == 0 { 14.0 } else { 26.0 }) + if label.is_empty() { 18.0 } else { 8.4 * label.chars().count() as f32 } + 12.0]
                                    .into_iter()
                                    .map(|body| {
                                        *edge += body;
                                        (j, *label, *edge - body, body)
                                    })
                                    .next()
                            })
                            .collect::<Vec<_>>()]
                        {
                            for (j, label, x, body) in segs.iter().copied() {
                                for idx in [segs.len() - 1 - j] {
                                    bt.push((rc(x, 46.0, x + body + 8.0, 78.0), 16 + idx, 0.0, 5.0, idx > 0, a));
                                    for (shade, ink) in [match (j, idx) {
                                        (0, _) => (0.9, 0.07),
                                        (_, 0) => (0.3, 0.96),
                                        _ => (0.17, 0.62),
                                    }] {
                                        for tone in [shade + 0.1 * anim[16 + idx][0]] {
                                            spr(rc(x, 46.0, x + body, 78.0), 6.0, gr(tone, a), 2);
                                            spr(rc(x + body, 46.0, x + body + 16.0, 78.0), 6.2, gr(tone, a), 28);
                                        }
                                        if idx > 0 {
                                            spr(rc(x + body + 3.0, 46.0, x + body + 19.0, 78.0), 6.1, gr(0.071, a), 28);
                                        }
                                        if label.is_empty() {
                                            spr(cs(x + if j == 0 { 23.0 } else { 35.0 }, 62.0, 18.0, 18.0), 6.5, gr(ink, a), 4);
                                        } else {
                                            txt(x + if j == 0 { 14.0 } else { 26.0 }, 62.0, 0, label, 14.0, 6.5, gr(ink, a));
                                        }
                                    }
                                }
                            }
                            for (x, body) in segs.last().map(|s| (s.2, s.3)).into_iter() {
                                for (nb, open) in [(rc(x + body + 30.0, 46.0, x + body + 108.0, 78.0), *xmenu as u8 as f32)] {
                                    bt.push((nb, 14, open, 5.0, true, a));
                                    for ink in [0.62 + 0.3 * anim[14][0].max(open)] {
                                        spr(cs(nb.min.x + 20.0, 62.0, 16.0, 16.0), 6.0, gr(ink, a), 9);
                                        txt(nb.min.x + 38.0, 62.0, 0, "New", 13.0, 6.5, gr(ink, a));
                                    }
                                    if *xmenu {
                                        spr(rc(nb.min.x - 8.0, 94.0, nb.min.x + 176.0, 256.0), 7.8, gr(0.0, a * 0.4), 0);
                                        spr(rc(nb.min.x, 86.0, nb.min.x + 168.0, 242.0), 8.0, gr(0.09, a), 0);
                                        spr(rc(nb.min.x, 86.0, nb.min.x + 168.0, 242.0), 8.1, gr(1.0, a * 0.05), 1);
                                        for (k, (label, glyph)) in [("Folder", 21_usize), ("File", 22), ("Image", 31), ("Video", 37)].into_iter().enumerate() {
                                            for (er, hv) in [(
                                                rc(nb.min.x + 6.0, 92.0 + 36.0 * k as f32, nb.min.x + 162.0, 124.0 + 36.0 * k as f32),
                                                anim[32 + k][0],
                                            )] {
                                                bt.push((er, 32 + k, 0.0, 8.2, true, a));
                                                spr(er, 8.15, gr(1.0, a * 0.03 * hv), 0);
                                                spr(cs(er.min.x + 22.0, er.center().y, 18.0, 18.0), 8.5, gr(0.62 + 0.31 * hv, a), glyph);
                                                txt(er.min.x + 44.0, er.center().y, 0, label, 14.0, 8.6, gr(0.7 + 0.25 * hv, a));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    spr(cs(left + 32.0, 120.0, 18.0, 18.0), 6.0, gr(0.46, a), 32);
    txt(left + 56.0, 120.0, 0, "NAME", 12.0, 6.5, gr(0.42, a));
    spr(cs(col - 24.0, 120.0, 18.0, 18.0), 6.0, gr(0.46, a), 29);
    txt(col, 120.0, 0, "SIZE", 12.0, 6.5, gr(0.42, a));
    spr(rc(left, 140.0, right, 141.0), 1.8, gr(0.1, a), 2);
    for (k, (name, folder, size, fin)) in xlist.iter().skip(*xscroll).take(vis).enumerate() {
        for (rr, on, img, vid) in [(
            rc(left, 152.0 + k as f32 * 36.0, right, 184.0 + k as f32 * 36.0),
            (*xsel == *xscroll + k + 1) as u8 as f32,
            std::path::Path::new(name.as_str())
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"].iter().any(|x| e.eq_ignore_ascii_case(x))),
            std::path::Path::new(name.as_str())
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["mp4", "mkv", "mov", "avi", "webm", "m4v"].iter().any(|x| e.eq_ignore_ascii_case(x))),
        )] {
            for hov in [(ptr.is_some_and(|c| rr.contains(c)) && !*xmenu) as u8 as f32] {
                spr(rr, 2.0, gr(1.0, a * (0.01 * hov + 0.012 * on)), 0);
                spr(rr, 2.1, gr(1.0, a * (0.012 * hov + 0.03 * on)), 1);
                spr(rc(rr.min.x + 8.0, rr.center().y - 9.0, rr.min.x + 10.0, rr.center().y + 9.0), 2.2, gr(0.93, a * on), 2);
                if *xscroll + k > 0 && *xscroll + k == xlist.partition_point(|e| e.1) {
                    spr(rc(rr.min.x + 16.0, rr.min.y - 2.0, rr.max.x - 16.0, rr.min.y - 1.0), 2.2, gr(0.157, a), 2);
                }
                spr(
                    cs(rr.min.x + 32.0, rr.center().y, 20.0, 20.0),
                    6.0,
                    gr(if *folder { 0.93 } else { 0.62 }, a),
                    match (*folder, img) {
                        (true, _) => 21,
                        (_, true) => 31,
                        (_, _) if vid => 37,
                        _ => 22,
                    },
                );
                txt(
                    rr.min.x + 56.0,
                    rr.center().y,
                    0,
                    &name[..name.char_indices().nth(60).map_or(name.len(), |c| c.0)],
                    15.0,
                    6.5,
                    gr(if *folder { 0.95 } else { 0.72 }, a),
                );
                for (id, label, glyph, x0, x1) in [
                    (26_usize, "Open", 20_usize, -710.0_f32, -634.0_f32),
                    (27, "Cut", 34, -626.0, -558.0),
                    (28, "Copy", 33, -550.0, -474.0),
                    (29, "Paste", 35, -466.0, -384.0),
                    (30, "Rename", 30, -376.0, -286.0),
                    (31, "Delete", 36, -278.0, -188.0),
                ]
                    .into_iter()
                    .filter(|_| hov > 0.0)
                {
                    for (btn, live) in [(rc(rr.max.x + x0, rr.center().y - 13.0, rr.max.x + x1, rr.center().y + 13.0), *folder || id != 26)] {
                        bt.push((btn, id, 0.0, 5.0, live, a));
                        for ink in [if live { 0.6 + 0.36 * anim[id][0] } else { 0.24 }] {
                            spr(btn, 2.3, gr(1.0, a * 0.012), 0);
                            spr(btn, 2.4, gr(1.0, a * (0.022 + 0.02 * anim[id][0] * live as u8 as f32)), 1);
                            spr(cs(btn.min.x + 18.0, btn.center().y, 14.0, 14.0), 6.0, gr(ink, a), glyph);
                            txt(btn.min.x + 32.0, btn.center().y, 0, label, 12.0, 6.5, gr(ink, a));
                        }
                    }
                }
                for (number, unit, f) in [[(1_u64 << 40, "TB"), (1 << 30, "GB"), (1 << 20, "MB"), (1 << 10, "KB")]
                    .iter()
                    .find(|(unit, _)| *size as f64 >= *unit as f64 * (1.0 - 1.0 / 20480.0))
                    .map_or((size.to_string(), "B"), |(unit, tag)| (format!("{:.1}", *size as f64 / *unit as f64), *tag))]
                .into_iter()
                .map(|(number, unit)| (number, unit, *fin as u8 as f32))
                {
                    txt(col, rr.center().y, 0, number.as_str(), 13.0, 6.5, gr(0.42 + 0.2 * f, a));
                    txt(col + 7.8 * (number.len() as f32 + 1.0), rr.center().y, 0, unit, 13.0, 6.5, gr(0.24 + 0.1 * f, a));
                }
            }
        }
    }
    for (cx, cy) in [((side + w - 4.0) / 2.0, (h + 152.0) / 2.0)].into_iter().filter(|_| xlist.is_empty()) {
        spr(cs(cx, cy - 24.0, 44.0, 44.0), 6.0, gr(0.16, a), 21);
        txt(cx, cy + 22.0, 2, "This folder is empty", 14.0, 6.5, gr(0.3, a));
    }
    for (track, thumb) in [(vis as f32 * 36.0 - 4.0, ((vis as f32 * 36.0 - 4.0) * vis as f32 / xlist.len().max(1) as f32).max(32.0))]
        .into_iter()
        .filter(|_| xlist.len() > vis)
    {
        for top in [152.0 + (track - thumb) * *xscroll as f32 / (xlist.len() - vis) as f32] {
            spr(rc(w - 38.0, top, w - 35.0, top + thumb), 2.3, gr(0.3, a), 2);
        }
    }
}
                                                    } else {
                                                      for kinds in [["Operating System", "Command Line Interface", "Graphical User Interface 2D", "Graphical User Interface 3D"]] {
                                                        spr(cs(side + 55.0, 56.0, 26.0, 26.0), 6.0, gr(0.93, a), if *view == 2 { 8 } else { 17 });
                                                        for tag in [if *view == 2 {
                                                            format!("{} projects", store.len())
                                                        } else {
                                                            store.get(*ed).map_or(String::new(), |pr| kinds[if pr.2 < 3 { pr.2 as usize - 1 } else { 1 + pr.3 as usize }].to_string())
                                                        }] {
                                                            txt(w - 44.0 - 7.8 * tag.len() as f32, 56.0, 0, tag.as_str(), 13.0, 6.5, gr(0.42, a));
                                                            for pr in store.get(*ed).into_iter().filter(|_| *view == 3) {
                                                                for (index, scale, tint) in [match pr.1 {
                                                                    Some((index, size)) => (index, size, 1.0),
                                                                    None => (if pr.2 < 3 { 11 + pr.2 as usize } else { 14 + pr.3 as usize }, bevy::math::Vec2::ONE, 0.93),
                                                                }] {
                                                                    for nx in [w - 68.0 - 7.8 * tag.len() as f32 - 9.0 * pr.0.chars().count() as f32] {
                                                                        spr(cs(nx - 18.0, 56.0, scale.x * 18.0, scale.y * 18.0), 6.0, gr(tint, a), index);
                                                                        txt(nx, 56.0, 0, pr.0.as_str(), 15.0, 6.5, gr(0.93, a));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        txt(
                                                            side + 82.0,
                                                            56.0,
                                                            0,
                                                            if *view == 2 { "Workspace" } else { "Settings" },
                                                            18.0,
                                                            6.5,
                                                            gr(0.93, a),
                                                        );
                                                        spr(rc(side + 40.0, 88.0, (w - 44.0).max(side + 41.0), 89.0), 1.8, gr(0.133, a), 2);
                                                        if *view == 2 {
                                                            for (i, pr) in store.iter().enumerate().skip(*lscroll * cols).take(rows * cols) {
                                                                for pc in [(side + 40.0 + (i % cols) as f32 * 396.0, 108.0 + (i / cols - *lscroll) as f32 * 160.0)]
                                                                    .map(|(x, y)| rc(x, y, x + 380.0, y + 144.0))
                                                                {
                                                                    for hv in [ptr.is_some_and(|c| pc.contains(c) && !*modal && !*picker) as u8 as f32] {
                                                                        spr(pc, 2.0, gr(0.102 + 0.012 * hv, a), 0);
                                                                        spr(pc, 2.1, gr(0.157 + 0.06 * hv, a), 1);
                                                                    }
                                                                    spr(rc(pc.min.x + 20.0, pc.min.y + 64.0, pc.max.x - 20.0, pc.min.y + 65.0), 2.2, gr(0.157, a), 2);
                                                                    match pr.1 {
                                                                        Some((index, size)) => spr(
                                                                            cs(pc.min.x + 35.0, pc.min.y + 32.0, size.x * 26.0, size.y * 26.0),
                                                                            6.0,
                                                                            gr(1.0, a),
                                                                            index,
                                                                        ),
                                                                        None => spr(cs(pc.min.x + 35.0, pc.min.y + 32.0, 22.0, 22.0), 6.0, gr(0.93, a), if pr.2 < 3 { 11 + pr.2 as usize } else { 14 + pr.3 as usize }),
                                                                    }
                                                                    txt(pc.min.x + 62.0, pc.min.y + 32.0, 0, pr.0.as_str(), 18.0, 6.5, gr(0.93, a));
                                                                    txt(
                                                                        pc.min.x + 22.0,
                                                                        pc.max.y - 34.0,
                                                                        0,
                                                                        kinds[if pr.2 < 3 { pr.2 as usize - 1 } else { 1 + pr.3 as usize }],
                                                                        10.0,
                                                                        6.5,
                                                                        gr(0.42, a),
                                                                    );
                                                                    for (k, lr) in [
                                                                        rc(pc.max.x - 172.0, pc.max.y - 52.0, pc.max.x - 136.0, pc.max.y - 16.0),
                                                                        rc(pc.max.x - 128.0, pc.max.y - 52.0, pc.max.x - 16.0, pc.max.y - 16.0),
                                                                    ]
                                                                    .into_iter()
                                                                    .enumerate()
                                                                    {
                                                                        bt.push((lr, 36 + 2 * i + k, 0.0, 5.0, !*modal && !*picker, a));
                                                                        spr(
                                                                            cs(if k == 0 { lr.center().x } else { lr.min.x + 22.0 }, lr.center().y, 18.0, 18.0),
                                                                            6.0,
                                                                            gr(0.93, a),
                                                                            if k == 0 { 17 } else { 18 },
                                                                        );
                                                                        if k == 1 {
                                                                            txt(lr.min.x + 40.0, lr.center().y, 0, "Launch", 14.0, 6.5, gr(0.93, a));
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            spr(cv, 1.6, gr(0.043, a), 0);
                                                            spr(cv, 1.7, gr(0.118, a), 1);
                                                        }
                                                      }
                                                    }
                                                }
                                            }
                                            if *view == 3 {
                                                for (i, (glyph, title, count)) in [
                                                    (19, "Bootstrapped", 0b1110),
                                                    (14, "Graphical User Interface", 0b1010),
                                                    (13, "Command Line Interface", 0b0110),
             (25, "Startup Line", 0b0100),
             (26, "Input", 0b0100),
                                                    (24, "Keyboard", 0b1110),
                                                ]
                                                .into_iter()
                                                .filter(|(_, _, shown)| shown >> store.get(*ed).map_or(0, |pr| pr.2) & 1 == 1)
                                                .enumerate()
                                                .map(|(i, (glyph, title, _))| (i, (glyph, title, 3 + (store.get(*ed).map_or(0, |pr| pr.2) != 3) as usize + (store.get(*ed).map_or(0, |pr| pr.2) == 2) as usize)))
                                                {
                                                    for r in [rc(
                                                        cv.min.x + 40.0 + 28.0 * i as f32,
                                                        cv.min.y + 16.0 + 72.0 * i as f32,
                                                        cv.min.x + 328.0 + 28.0 * i as f32,
                                                        cv.min.y + 68.0 + 72.0 * i as f32,
                                                    )] {
                                                        spr(
                                                            rc(r.min.x - 16.0 - 28.0 * (i > 0) as u8 as f32, r.center().y - 1.0, r.min.x, r.center().y + 1.0),
                                                            7.8,
                                                            gr(0.26, fades[0]),
                                                            2,
                                                        );
                                                        if i + 1 < count {
                                                            spr(rc(r.min.x - 17.0, r.center().y - 1.0, r.min.x - 15.0, r.center().y + 73.0), 7.8, gr(0.26, fades[0]), 2);
                                                        }
                                                        spr(r.inflate(3.0), 7.9, gr(1.0, 0.01 * fades[0]), 0);
                                                        spr(r, 8.0, gr(0.102, fades[0]), 0);
                                                        spr(r, 8.05, gr(0.2, fades[0]), 1);
                                                        for tile in [rc(r.min.x + 10.0, r.min.y + 10.0, r.min.x + 42.0, r.min.y + 42.0)] {
                                                            spr(tile, 8.1, gr(0.157, fades[0]), 0);
                                                            spr(tile, 8.15, gr(0.26, fades[0]), 1);
                                                            spr(cs(tile.center().x, tile.center().y, 18.0, 18.0), 8.2, gr(0.93, fades[0]), glyph);
                                                        }
                                                        if glyph > 24 {
                     for (id, caption, hint, text) in store
                         .get(*ed)
                         .map(|pr| {
                             if glyph == 25 {
                                 (11, "STARTUP LINE", "Text shown on launch", pr.5[0].as_str())
                             } else {
                                 (12, "INPUT PROMPT", "No prompt", pr.5[1].as_str())
                             }
                         })
                         .into_iter()
                     {
                         bt.push((r, id, (*edit as usize == id) as u8 as f32, 8.02, true, fades[0]));
                         spr(r, 8.06, gr(0.6, fades[0] * anim[id][3]), 1);
                         txt(r.min.x + 54.0, r.min.y + 17.0, 0, caption, 10.0, 8.3, gr(0.42, fades[0]));
                         txt(
                             r.min.x + 54.0,
                             r.min.y + 35.0,
                             0,
                             if text.is_empty() { hint } else { text },
                             14.0,
                             8.3,
                             gr(if text.is_empty() { 0.34 } else { 0.93 }, fades[0]),
                         );
                         if *edit as usize == id {
                             spr(
                                 rc(
                                     r.min.x + 54.0 + text.len() as f32 * 8.4,
                                     r.min.y + 27.0,
                                     r.min.x + 55.5 + text.len() as f32 * 8.4,
                                     r.min.y + 43.0,
                                 ),
                                 8.3,
                                 gr(0.93, fades[0]),
                                 2,
                             );
                         }
                     }
                 } else {
                     txt(r.min.x + 54.0, r.center().y, 0, title, 15.0, 8.3, gr(0.93, fades[0]));
                 }
                                                    }
                                                }
                                                for (pr, pw) in store
                                                    .get(*ed)
                                                    .into_iter()
                                                    .map(|pr| (pr, (cv.max.x - cv.min.x - 560.0).min(760.0).min((cv.height() - 90.0) * w / h)))
                                                    .filter(|(_, pw)| *pw >= 320.0)
                                                {
                                                    for (pv, s) in [(
                                                        rc(cv.min.x + 520.0, cv.min.y + 40.0, cv.min.x + 520.0 + pw, cv.min.y + 40.0 + pw * h / w),
                                                        pw / w,
                                                    )] {
                                                        txt(pv.min.x, cv.min.y + 24.0, 0, "PREVIEW", 10.0, 8.3, gr(0.42, fades[0]));
                                                        for size in [format!("{} x {}", w as u32, h as u32)] {
                                                            txt(pv.max.x - 6.0 * size.len() as f32, cv.min.y + 24.0, 0, size.as_str(), 10.0, 8.3, gr(0.42, fades[0]));
                                                        }
                                                        spr(pv.inflate(3.0), 7.9, gr(1.0, 0.01 * fades[0]), 0);
                                                        spr(pv, 8.0, gr(if pr.2 == 2 { 0.039 } else { 1.0 }, fades[0]), 2);
                                                        for edge in [
                                                            rc(pv.min.x - 1.0, pv.min.y - 1.0, pv.max.x + 1.0, pv.min.y),
                                                            rc(pv.min.x - 1.0, pv.max.y, pv.max.x + 1.0, pv.max.y + 1.0),
                                                            rc(pv.min.x - 1.0, pv.min.y, pv.min.x, pv.max.y),
                                                            rc(pv.max.x, pv.min.y, pv.max.x + 1.0, pv.max.y),
                                                        ] {
                                                            spr(edge, 8.4, gr(0.26, fades[0]), 2);
                                                        }
                                                        if pr.2 == 2 {
                                                            txt(pv.min.x + 24.0 * s, pv.min.y + 39.0 * s, 0, pr.5[0].as_str(), 30.0 * s, 8.3, gr(0.9, fades[0]));
                                                            txt(
                                                                pv.min.x + 24.0 * s,
                                                                pv.min.y + (39.0 + 30.0 * !pr.5[0].is_empty() as u8 as f32) * s,
                                                                0,
                                                                pr.5[1].as_str(),
                                                                30.0 * s,
                                                                8.3,
                                                                gr(0.9, fades[0]),
                                                            );
                                                            spr(
                                                                rc(
                                                                    pv.min.x + (24.0 + 18.0 * pr.5[1].len() as f32) * s,
                                                                    pv.min.y + (24.0 + 30.0 * !pr.5[0].is_empty() as u8 as f32) * s,
                                                                    pv.min.x + (39.0 + 18.0 * pr.5[1].len() as f32) * s,
                                                                    pv.min.y + (51.0 + 30.0 * !pr.5[0].is_empty() as u8 as f32) * s,
                                                                ),
                                                                8.2,
                                                                gr(0.9, fades[0]),
                                                                2,
                                                            );
                                                        } else {
                                                            for (x, y) in (0..16_usize)
                                                                .flat_map(|x| (0..(pv.height() * 16.0 / pv.width()).ceil() as usize).map(move |y| (x, y)))
                                                                .filter(|(x, y)| (x + y) % 2 == 1)
                                                            {
                                                                spr(
                                                                    rc(
                                                                        pv.min.x + x as f32 * pv.width() / 16.0,
                                                                        pv.min.y + y as f32 * pv.width() / 16.0,
                                                                        pv.min.x + (x + 1) as f32 * pv.width() / 16.0,
                                                                        (pv.min.y + (y + 1) as f32 * pv.width() / 16.0).min(pv.max.y),
                                                                    ),
                                                                    8.1,
                                                                    gr(0.902, fades[0]),
                                                                    2,
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if *modal {
                                                for a in [fades[1]] {
                                                    spr(rc(0.0, 0.0, w, h), 20.0, bevy::color::Color::srgba(0.0, 0.0, 0.0, 0.62 * a), 2);
                                                    spr(mrect.inflate(18.0), 20.4, gr(0.0, 0.16 * a), 0);
                                                    spr(mrect.inflate(9.0), 20.7, gr(0.0, 0.24 * a), 0);
                                                    spr(mrect, 21.0, gr(0.078, a), 0);
                                                    spr(mrect, 21.5, gr(0.176, a), 1);
                                                    spr(cs(mrect.min.x + 35.0, mrect.min.y + 32.0, 26.0, 26.0), 23.0, gr(0.93, a), 10);
                                                    txt(mrect.min.x + 62.0, mrect.min.y + 32.0, 0, "Create project", 18.0, 23.0, gr(0.93, a));
                                                    for dy in [64.0, 416.0] {
                                                        spr(rc(mrect.min.x + 20.0, mrect.min.y + dy, mrect.max.x - 20.0, mrect.min.y + dy + 1.0), 23.0, gr(0.157, a), 2);
                                                    }
                                                    for (dy, label) in [(80.0, "ICON"), (176.0, "NAME"), (244.0, "TYPE")] {
                                                        txt(mrect.min.x + 24.0, mrect.min.y + dy, 1, label, 11.0, 23.0, gr(0.42, a));
                                                    }
                                                    for ir in [rc(mrect.min.x + 24.0, mrect.min.y + 100.0, mrect.min.x + 88.0, mrect.min.y + 164.0)] {
                                                        bt.push((ir, 4, 0.0, 24.0, !*picker, a));
                                                        match *icon {
                                                            Some((index, size)) => spr(
                                                                cs(ir.center().x, ir.center().y, size.x * 48.0, size.y * 48.0),
                                                                25.0,
                                                                gr(1.0, a),
                                                                index,
                                                            ),
                                                            None => spr(cs(ir.center().x, ir.center().y, 28.0, 28.0), 25.0, gr(0.5 + 0.3 * anim[4][0], a), 11),
                                                        }
                                                        txt(
                                                            ir.max.x + 16.0,
                                                            ir.center().y,
                                                            0,
                                                            if icon.is_some() { "PNG selected - click to change" } else { "Click to choose a PNG file" },
                                                            13.0,
                                                            23.0,
                                                            gr(0.5, a),
                                                        );
                                                    }
                                                    for fr in [rc(mrect.min.x + 24.0, mrect.min.y + 196.0, mrect.max.x - 24.0, mrect.min.y + 232.0)] {
                                                        spr(fr, 23.0, gr(0.039, a), 0);
                                                        spr(fr, 23.2, gr(if *picker { 0.2 } else { 0.5 }, a), 1);
                                                        txt(
                                                            fr.min.x + 12.0,
                                                            fr.center().y,
                                                            0,
                                                            if name.is_empty() { "Project name" } else { name.as_str() },
                                                            15.0,
                                                            24.5,
                                                            gr(if name.is_empty() { 0.34 } else { 0.93 }, a),
                                                        );
                                                        if !*picker {
                                                            spr(
                                                                rc(
                                                                    fr.min.x + 12.0 + name.len() as f32 * 9.0,
                                                                    fr.center().y - 9.0,
                                                                    fr.min.x + 13.5 + name.len() as f32 * 9.0,
                                                                    fr.center().y + 9.0,
                                                                ),
                                                                24.6,
                                                                gr(0.93, a),
                                                                2,
                                                            );
                                                        }
                                                    }
                                                    for (k, (index, label)) in [(12, "Operating System"), (13, "Command Line Interface"), (14, "Graphical User Interface")]
                                                        .into_iter()
                                                        .enumerate()
                                                    {
                                                        for tr in [rc(
                                                            mrect.min.x + 24.0,
                                                            mrect.min.y + 264.0 + k as f32 * 48.0,
                                                            mrect.max.x - 24.0,
                                                            mrect.min.y + 304.0 + k as f32 * 48.0,
                                                        )] {
                                                            bt.push((tr, 5 + k, (*kind as usize == k + 1) as u8 as f32, 24.0, !*picker, a));
                                                            spr(
                                                                cs(tr.min.x + 26.0, tr.center().y, 22.0, 22.0),
                                                                25.0,
                                                                gr(0.6 + 0.33 * anim[5 + k][0].max(anim[5 + k][3]), a),
                                                                index,
                                                            );
                                                            txt(
                                                                tr.min.x + 54.0,
                                                                tr.center().y,
                                                                0,
                                                                label,
                                                                14.0,
                                                                25.5,
                                                                gr(0.6 + 0.33 * anim[5 + k][0].max(anim[5 + k][3]), a),
                                                            );
                                                        }
                                                    }
                                                    if *kind == 3 {
                                                        for (k, (index, label)) in [(15, "2D"), (16, "3D")].into_iter().enumerate() {
                                                            for dr in [rc(
                                                                mrect.min.x + 24.0 + k as f32 * 132.0,
                                                                mrect.min.y + 432.0,
                                                                mrect.min.x + 144.0 + k as f32 * 132.0,
                                                                mrect.min.y + 468.0,
                                                            )] {
                                                                bt.push((dr, 8 + k, (*dim as usize == k + 1) as u8 as f32, 24.0, !*picker, a));
                                                                spr(
                                                                    cs(dr.min.x + 24.0, dr.center().y, 18.0, 18.0),
                                                                    25.0,
                                                                    gr(0.6 + 0.33 * anim[8 + k][0].max(anim[8 + k][3]), a),
                                                                    index,
                                                                );
                                                                txt(
                                                                    dr.min.x + 44.0,
                                                                    dr.center().y,
                                                                    0,
                                                                    label,
                                                                    14.0,
                                                                    25.5,
                                                                    gr(0.6 + 0.33 * anim[8 + k][0].max(anim[8 + k][3]), a),
                                                                );
                                                            }
                                                        }
                                                    }
                                                    for (sr, ok) in [(
                                                        rc(mrect.max.x - 136.0, mrect.min.y + 432.0, mrect.max.x - 24.0, mrect.min.y + 468.0),
                                                        !name.is_empty() && *kind > 0 && (*kind < 3 || *dim > 0),
                                                    )] {
                                                        bt.push((sr, 10, ok as u8 as f32, 24.0, ok && !*picker, a));
                                                        txt(
                                                            sr.center().x,
                                                            sr.center().y,
                                                            2,
                                                            "SAVE",
                                                            14.0,
                                                            25.5,
                                                            gr(0.93 - 0.86 * anim[10][3], a * (0.4 + 0.6 * anim[10][3])),
                                                        );
                                                    }
                                                }
                                            }
                                            if *picker {
                                                for a in [fades[2]] {
                                                    spr(rc(0.0, 0.0, w, h), 30.0, bevy::color::Color::srgba(0.0, 0.0, 0.0, 0.5 * a), 2);
                                                    spr(prect.inflate(18.0), 30.4, gr(0.0, 0.16 * a), 0);
                                                    spr(prect.inflate(9.0), 30.7, gr(0.0, 0.24 * a), 0);
                                                    spr(prect, 31.0, gr(0.078, a), 0);
                                                    spr(prect, 31.5, gr(0.176, a), 1);
                                                    spr(cs(prect.min.x + 35.0, prect.min.y + 32.0, 26.0, 26.0), 33.0, gr(0.93, a), 20);
                                                    txt(prect.min.x + 62.0, prect.min.y + 32.0, 0, "Choose PNG icon", 18.0, 33.0, gr(0.93, a));
                                                    spr(rc(prect.min.x + 20.0, prect.min.y + 64.0, prect.max.x - 20.0, prect.min.y + 65.0), 33.0, gr(0.157, a), 2);
                                                    for path in [dir.to_string_lossy()] {
                                                        txt(
                                                            prect.min.x + 24.0,
                                                            prect.min.y + 76.0,
                                                            1,
                                                            &path[path.char_indices().rev().nth(63).map_or(0, |c| c.0)..],
                                                            12.0,
                                                            33.0,
                                                            gr(0.42, a),
                                                        );
                                                    }
                                                    for k in 0..12usize {
                                                        if *pscroll + k <= entries.len() {
                                                            for (label, folder, rr) in [(
                                                                if *pscroll + k == 0 { ".." } else { entries[*pscroll + k - 1].0.as_str() },
                                                                *pscroll + k == 0 || entries[(*pscroll + k).max(1) - 1].1,
                                                                rc(
                                                                    prect.min.x + 16.0,
                                                                    prect.min.y + 100.0 + k as f32 * 26.0,
                                                                    prect.max.x - 16.0,
                                                                    prect.min.y + 126.0 + k as f32 * 26.0,
                                                                ),
                                                            )] {
                                                                bt.push((rr, 16 + k, 0.0, 33.0, true, a));
                                                                spr(cs(rr.min.x + 22.0, rr.center().y, 16.0, 16.0), 34.0, gr(0.6 + 0.33 * anim[16 + k][0].max(anim[16 + k][1]), a), if *pscroll + k == 0 { 23 } else if folder { 21 } else { 22 });
                                                                txt(
                                                                    rr.min.x + 40.0,
                                                                    rr.center().y,
                                                                    0,
                                                                    &label[..label.char_indices().nth(52).map_or(label.len(), |c| c.0)],
                                                                    14.0,
                                                                    34.5,
                                                                    gr(0.6 + 0.33 * anim[16 + k][0].max(anim[16 + k][1]), a),
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            for (r, id, _, z, _, al) in bt.iter() {
                                                for ([hv, fc, pr, sl], ga) in [(
                                                    anim[*id],
                                                    if (16..36).contains(id) { *al * anim[*id][0].max(anim[*id][1]) } else { *al },
                                                )] {
                                                    if ga > 0.0 {
                                                        spr(
                                                            *r,
                                                            *z,
                                                            gr(
                                                                1.0,
                                                                if *id == 10 {
                                                                    (0.012 + 0.9 * sl + 0.07 * hv * sl) * (1.0 - 0.2 * pr) * ga
                                                                } else {
                                                                    ((0.012 + 0.014 * hv + 0.026 * sl) * (1.0 - pr) + 0.07 * pr) * ga
                                                                },
                                                            ),
                                                            0,
                                                        );
                                                        spr(*r, *z + 0.01, gr(1.0, (0.02 + 0.03 * hv + 0.02 * sl) * ga), 1);
                                                    }
                                                    if fc > 0.0 {
                                                        spr(r.inflate(2.0), *z + 0.02, gr(0.55, fc * al), 1);
                                                    }
                                                }
                                            }
                                        }
                                        if *view == 4 {
                                            spr(rc(0.0, 0.0, w, h), 1.0, gr(1.0, 1.0), 38);
                                        }
                                    });
                                    sit.for_each(|(_, _, v)| {
                                        Some(v).iter_mut().for_each(|v| {
                                            bevy::ecs::change_detection::DetectChangesMut::set_if_neq(v, bevy::camera::visibility::Visibility::Hidden);
                                        });
                                    });
                                    tit.for_each(|(.., v)| {
                                        Some(v).iter_mut().for_each(|v| {
                                            bevy::ecs::change_detection::DetectChangesMut::set_if_neq(v, bevy::camera::visibility::Visibility::Hidden);
                                        });
                                    });
                                });
for ke in [keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) && !*hud && *view != 4] {
for (mdn, mup, mhd) in [(
    mdn || (ke && keys.just_pressed(bevy::input::keyboard::KeyCode::KeyE)),
    mup || (keys.just_released(bevy::input::keyboard::KeyCode::KeyE) && !*hud && *view != 4),
    mhd || (ke && keys.pressed(bevy::input::keyboard::KeyCode::KeyE)),
)] {
                            *kact = *kact
    && !mouse.just_pressed(bevy::input::mouse::MouseButton::Left)
    && window.cursor_position().is_none_or(|c| c == *km);
*moving |= window.cursor_position().is_some_and(|c| c != *km);
*km = window.cursor_position().unwrap_or(*km);
for dir in [bevy::math::Vec2::new(
    keys.pressed(bevy::input::keyboard::KeyCode::KeyD) as u8 as f32 - keys.pressed(bevy::input::keyboard::KeyCode::KeyA) as u8 as f32,
    keys.pressed(bevy::input::keyboard::KeyCode::KeyS) as u8 as f32 - keys.pressed(bevy::input::keyboard::KeyCode::KeyW) as u8 as f32,
) * (keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) && !*hud && *view != 4) as u8 as f32]
{
    *kh = (*kh + dt) * (dir != bevy::math::Vec2::ZERO) as u8 as f32;
    *kc = if dir != bevy::math::Vec2::ZERO && !*kact {
        window.cursor_position().unwrap_or(bevy::math::Vec2::new(w / 2.0, h / 2.0))
    } else {
        *kc
    };
    *kact |= dir != bevy::math::Vec2::ZERO;
    *kc = (*kc + dir.normalize_or_zero() * (140.0 + 1500.0 * (*kh / 0.9).min(1.0).powi(2)) * dt)
        .clamp(bevy::math::Vec2::ZERO, bevy::math::Vec2::new(w - 1.0, h - 1.0));
    *moving |= dir != bevy::math::Vec2::ZERO;
}
*ptr = if *kact { Some(*kc) } else { window.cursor_position() };
xjob.as_mut().into_iter().for_each(|(rx, job, working)| {
    job.stop.fetch_or(*view != 5, std::sync::atomic::Ordering::Relaxed);
    *working = job.todo.load(std::sync::atomic::Ordering::Acquire) > 0 && !job.stop.load(std::sync::atomic::Ordering::Relaxed);
    *moving |= rx
        .try_iter()
        .map(|(row, path, total, top)| {
            (xcache.len() > 400_000).then(|| xcache.clear());
            xcache.insert(path, total);
            xlist.get_mut(row).filter(|_| top).into_iter().for_each(|e| {
                e.2 = total;
                e.3 = true;
            });
        })
        .count()
        > 0;
    *moving |= xlist
        .iter_mut()
        .zip(job.live.iter())
        .filter(|(e, live)| e.1 && !e.3 && e.2 != live.load(std::sync::atomic::Ordering::Relaxed))
        .map(|(e, live)| e.2 = live.load(std::sync::atomic::Ordering::Relaxed))
        .count()
        > 0;
});
*xhit = ptr
    .filter(|c| {
        *view == 5
            && c.x > side + 48.0
            && c.x < w - 52.0
            && !*xmenu
            && c.y >= 152.0
            && (c.y - 152.0) / 36.0 < vis as f32
    })
    .map(|c| *xscroll + ((c.y - 152.0) / 36.0) as usize)
    .filter(|row| *row < xlist.len());
*wacc += 14.0
    * dt
    * (*kact && keys.pressed(bevy::input::keyboard::KeyCode::ControlLeft) && *view == 5 && !*hud) as u8 as f32
    * ((kc.y < 176.0 && keys.pressed(bevy::input::keyboard::KeyCode::KeyW)) as u8 as f32
        - (kc.y > 128.0 + 36.0 * vis as f32 && keys.pressed(bevy::input::keyboard::KeyCode::KeyS)) as u8 as f32);
*hover = ptr
                                .filter(|_| window.focused && !*hud)
                                .and_then(|c| bt.iter().find(|b| b.4 && b.0.contains(c)).map(|b| b.1));
                            *focus = if keys.just_pressed(bevy::input::keyboard::KeyCode::Tab) {
                                Some((
                                    bt.iter().filter(|b| b.4).count(),
                                    keys.any_pressed([
                                        bevy::input::keyboard::KeyCode::ShiftLeft,
                                        bevy::input::keyboard::KeyCode::ShiftRight,
                                    ]),
                                ))
                                .filter(|(n, _)| *n > 0)
                                .and_then(|(n, sh)| {
                                    bt.iter().filter(|b| b.4).map(|b| b.1).nth(
                                        (focus
                                            .and_then(|f| bt.iter().filter(|b| b.4).position(|b| b.1 == f))
                                            .unwrap_or(if sh { 0 } else { n - 1 })
                                            + if sh { n - 1 } else { 1 })
                                            % n,
                                    )
                                })
                            } else {
                                focus.filter(|f| bt.iter().any(|b| b.4 && b.1 == *f))
                            };
                            *press = if mdn {
                                hover.map(|i| (i, time.elapsed_secs()))
                            } else if mhd || mup {
                                *press
                            } else {
                                None
                            };
                            Some(
                                press
                                    .filter(|&(i, t0)| mup && *hover == Some(i) && time.elapsed_secs() - t0 <= 1.5)
                                    .map(|p| p.0)
                                    .or(focus.filter(|_| {
                                        keys.just_pressed(bevy::input::keyboard::KeyCode::Enter)
                                            || (keys.just_pressed(bevy::input::keyboard::KeyCode::Space)
                                                && !(*modal && !*picker)
                                                && *edit == 0) })),
                            )
                            .into_iter()
                            .for_each(|act| {
                                *press = press.filter(|_| !mup);
                                *moving |= act.is_some() || mdn || mup || keys.get_just_pressed().len() > 0;
                                if keys.just_pressed(bevy::input::keyboard::KeyCode::Escape)
                                    || (mdn
                                        && ptr
                                            .is_some_and(|c| !(if *picker { prect } else { mrect }).contains(c)))
                                {
                                    *modal &= *picker;
                                    *picker = false;
                                }
                                *xmenu &= !(keys.just_pressed(bevy::input::keyboard::KeyCode::Escape)
                                    || (mdn && !hover.is_some_and(|id| id == 14 || (32..36).contains(&id))));
                                xhit.filter(|_| mdn && hover.is_none()).into_iter()
                                    .for_each(|row| {
                                        *xsel = row + 1;
                                        if xlist[row].1 {
                                            xdir.push(&xlist[row].0);
                                            *xdirty = true;
                                            *xscroll = 0;
                                            *xsel = 0;
                                        }
                                    });
                                *edit = match act {
                                    Some(i @ 11..=12) if i != *edit as usize => i as u8,
                                    _ => {
                                        *edit
                                            * !(keys.just_pressed(bevy::input::keyboard::KeyCode::Escape)
                                                || keys.just_pressed(bevy::input::keyboard::KeyCode::Enter)
                                                || *view != 3
                                                || (mdn
                                                    && !ptr.is_some_and(|c| {
                                                        bt.iter().any(|b| b.1 == *edit as usize && b.0.contains(c))
                                                    }))) as u8
                                    }
                                };
                                match act {
                                    Some(i @ 0..=2) => *view = i as u8,
                                    Some(3) => *modal = true,
                                    Some(13) => {
                                        *view = 5;
                                        *xmenu = false;
                                        *xdirty = true;
                                        *xscroll = 0;
                                        *xsel = 0;
                                        *xdir = std::env::var_os("HOME")
                                            .or_else(|| std::env::var_os("USERPROFILE"))
                                            .map_or(".".into(), Into::into);
                                    }
                                    Some(26) if *view == 5 => {
                                        xhit.and_then(|row| xlist.get(row))
                                            .filter(|e| e.1)
                                            .map(|e| e.0.clone())
                                            .into_iter()
                                            .for_each(|name| {
                                                xdir.push(name);
                                                *xdirty = true;
                                                *xscroll = 0;
                                                *xsel = 0;
                                            });
                                    }
                                    Some(27) if *view == 5 => {}
                                    Some(14) if *view == 5 => *xmenu = !*xmenu,
                                    Some(i @ 32..=35) if *view == 5 => {
                                        for (name, ext, data) in [[
                                            ("New folder", "", None),
                                            ("New file", ".txt", Some(&b""[..])),
                                            ("New image", ".png", Some(&b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0d\x49\x44\x41\x54\x78\x9c\x63\x60\x60\x60\x60\x00\x00\x00\x05\x00\x01\xa5\xf6\x45\x40\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82"[..])),
                                            ("New video", ".mp4", Some(&b""[..])),
                                        ][i - 32]] {
                                            (1_u32..)
                                                .map(|n| {
                                                    xdir.join(if n == 1 { format!("{name}{ext}") } else { format!("{name} {n}{ext}") })
                                                })
                                                .find(|path| !path.exists())
                                                .into_iter()
                                                .for_each(|path| {
                                                    data.map_or_else(|| std::fs::create_dir(&path), |bytes| std::fs::write(&path, bytes)).ok();
                                                });
                                        }
                                        *xmenu = false;
                                        *xdirty = true;
                                    }
Some(i @ 16..=25) if *view == 5 => {
                                        xdir.ancestors()
                                            .nth(i - 16)
                                            .map(std::path::Path::to_path_buf)
                                            .into_iter()
                                            .for_each(|up| *xdir = up);
                                        *xdirty = true;
                                        *xscroll = 0;
                                        *xsel = 0;
                                    }
                                    Some(4) => {
                                        *picker = true;
                                        *dirty = true;
                                        *pscroll = 0;
                                        if dir.as_os_str().is_empty() {
                                            *dir = std::env::var_os("HOME")
                                                .or_else(|| std::env::var_os("USERPROFILE"))
                                                .map_or(".".into(), Into::into);
                                        }
                                    }
                                    Some(i @ 5..=7) => {
                                        *kind = (i - 4) as u8;
                                        *dim *= (i == 7) as u8;
                                    }
                                    Some(i @ 8..=9) => *dim = (i - 7) as u8,
                                    Some(10) => {
                                        for id in [std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_nanos() as u64]
                                        {
                                            Some(db.begin_write().unwrap()).into_iter().for_each(|tx| {
                                                Some(tx.open_table(redb::TableDefinition::<&str, &[u8]>::new("fs")).unwrap())
                                                    .iter_mut()
                                                    .for_each(|table| {
                                                        table
                                                            .insert(
                                                                format!("/projects/{id:016x}/meta").as_str(),
                                                                [*kind, *dim].into_iter().chain(name.bytes()).collect::<Vec<u8>>().as_slice(),
                                                            )
                                                            .unwrap();
                                                        if !pending.is_empty() {
                                                            table
                                                                .insert(format!("/projects/{id:016x}/icon.png").as_str(), pending.as_slice())
                                                                .unwrap();
                                                        }
                                                    });
                                                tx.commit().unwrap();
                                            });
                                            pending.clear();
                                            store.push((std::mem::take(name), icon.take(), *kind, *dim, id, [String::new(), String::from("> ")]));
                                        }
                                        *kind = 0;
                                        *dim = 0;
                                        *modal = false;
                                        *picker = false;
                                    }
                                    Some(i @ 16..=27) => {
                                        match (*pscroll + i - 16).checked_sub(1).and_then(|e| entries.get(e)) {
                                            None => {
                                                dir.pop();
                                                *dirty = true;
                                                *pscroll = 0;
                                            }
                                            Some((n, true)) => {
                                                dir.push(n);
                                                *dirty = true;
                                                *pscroll = 0;
                                            }
                                            Some((n, false)) => {
                                                std::fs::read(dir.join(n)).ok().into_iter().for_each(|bytes| {
                                                    jobs.push((usize::MAX, bytes));
                                                    *picker = false;
                                                });
                                            }
                                        }
                                    }
                                    Some(i @ 36..) => {
                                        for (project, gear) in [((i - 36) / 2, i % 2 == 0)] {
                                            *ed = project;
                                            if gear {
                                                *view = 3;
                                            } else {
                                                *view = 4;
                                                for (width, height, cli) in [(w as usize, h as usize, store[project].2 == 2)] {
                                                    for (pixels, generation, on, (tx, rx), texts) in [(
                                                        (0..width * height)
                                                            .map(|_| std::sync::atomic::AtomicU32::new(0))
                                                            .collect::<std::sync::Arc<[std::sync::atomic::AtomicU32]>>(),
                                                        std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                                                        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
                                                        std::sync::mpsc::channel::<u8>(),
                                                        store[project].5.clone(),
                                                    )] {
                                                        icons[38] = images.add(bevy::image::Image::new(
                                                            bevy::render::render_resource::Extent3d {
                                                                width: width as u32,
                                                                height: height as u32,
                                                                depth_or_array_layers: 1,
                                                            },
                                                            bevy::render::render_resource::TextureDimension::D2,
                                                            [0, 0, 0, 255].repeat(width * height),
                                                            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                                                            bevy::asset::RenderAssetUsages::MAIN_WORLD
                                                                | bevy::asset::RenderAssetUsages::RENDER_WORLD,
                                                        ));
                                                        *machine = Some((
                                                            pixels.clone(),
                                                            generation.clone(),
                                                            on.clone(),
                                                            std::thread::spawn(move || {
                                                                if cli {
                                                                    kernel::cli(
                                                                        &pixels,
                                                                        width,
                                                                        || {
                                                                            generation.fetch_add(1, std::sync::atomic::Ordering::Release);
                                                                        },
                                                                        || {
                                                                            std::thread::park_timeout(std::time::Duration::from_millis(530));
                                                                            on.load(std::sync::atomic::Ordering::Acquire)
                                                                        },
                                                                        || rx.try_recv().ok(),
                                                                        texts[0].as_bytes(),
                                                                        texts[1].as_bytes(),
                                                                    );
                                                                } else {
                                                                    kernel::boot(
                                                                        &pixels,
                                                                        width,
                                                                        || {
                                                                            generation.fetch_add(1, std::sync::atomic::Ordering::Release);
                                                                        },
                                                                        || {
                                                                            std::thread::park();
                                                                            on.load(std::sync::atomic::Ordering::Acquire)
                                                                        },
                                                                    );
                                                                }
                                                            })
                                                            .thread()
                                                            .clone(),
                                                            0,
                                                            tx,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            });
                            if *xdirty {
                                *xdirty = false;
                                *xlist = std::fs::read_dir(&*xdir)
                                    .into_iter()
                                    .flatten()
                                    .flatten()
                                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                                    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().metadata().ok().map(|m| (m.is_dir(), m.len()))))
                                    .map(|(name, meta)| match meta {
                                        Some((true, _)) => match xcache.get(&xdir.join(&name)) {
                                            Some(&size) => (name, true, size, true),
                                            None => (name, true, 0, false),
                                        },
                                        Some((_, len)) => (name, false, len, true),
                                        None => (name, false, 0, true),
                                    })
                                    .collect();
                                xlist.sort_by_cached_key(|e| (!e.1, e.0.to_lowercase()));
                                xjob.as_ref().into_iter().for_each(|(_, job, _)| job.stop.store(true, std::sync::atomic::Ordering::Relaxed));
                                for ((tx, rx), roots) in [(
                                    std::sync::mpsc::channel(),
                                    xlist
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, e)| e.1)
                                        .map(|(row, e)| {
                                            std::sync::Arc::new(Dir {
                                                path: xdir.join(&e.0),
                                                parent: None,
                                                row,
                                                sum: std::sync::atomic::AtomicU64::new(0),
                                                open: std::sync::atomic::AtomicUsize::new(1),
                                            })
                                        })
                                        .rev()
                                        .collect::<Vec<_>>(),
                                )] {
                                    for (job, workers, rx) in [(
                                        std::sync::Arc::new(Walk {
                                            todo: std::sync::atomic::AtomicUsize::new(roots.len()),
                                            stop: std::sync::atomic::AtomicBool::new(false),
                                            live: xlist.iter().map(|_| std::sync::atomic::AtomicU64::new(0)).collect(),
                                            queue: std::sync::Mutex::new(roots),
                                        }),
                                        std::thread::available_parallelism().map_or(2, std::num::NonZero::get).min(8),
                                        rx,
                                    )] {
                                        (0..workers * xlist.iter().any(|e| e.1) as usize).for_each(|_| {
                                            for (job, tx) in [(job.clone(), tx.clone())] {
                                                std::thread::spawn(move || walk(&job, &tx));
                                            }
                                        });
                                        *xjob = Some((rx, job, true));
                                    }
                                }
                                *moving = true;
                            }
                            if *dirty {
                                *dirty = false;
                                *entries = std::fs::read_dir(&*dir)
                                    .into_iter()
                                    .flatten()
                                    .flatten()
                                    .filter_map(|e| {
                                        Some(e.path()).and_then(|p| {
                                            e.file_name()
                                                .into_string()
                                                .ok()
                                                .filter(|n| !n.starts_with('.'))
                                                .map(|n| (n, p.is_dir()))
                                                .filter(|(_, d)| {
                                                    *d || (p.is_file()
                                                        && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
                                                })
                                        })
                                    })
                                    .collect();
                                entries.sort_by_cached_key(|e| (!e.1, e.0.to_lowercase()));
                                *moving = true;
                            }
                            Some(wacc.trunc())
                                .filter(|steps| *steps != 0.0)
                                .into_iter()
                                .for_each(|steps| {
                                    *wacc -= steps;
                                    *moving = true;
                                    if *picker {
                                        *pscroll = ((*pscroll as f32 - steps).max(0.0) as usize)
                                            .min((entries.len() + 1).saturating_sub(12));
                                    } else if *view == 2 && !*modal {
                                        *lscroll = ((*lscroll as f32 - steps).max(0.0) as usize)
                                            .min(store.len().div_ceil(cols).saturating_sub(rows));
                                    } else if *view == 5 {
                                        *xscroll = ((*xscroll as f32 - steps).max(0.0) as usize).min(xlist.len().saturating_sub(vis));
                                    }
                                });
                            *esc = if !keys.pressed(bevy::input::keyboard::KeyCode::Escape) {
                                0.0
                            } else if *esc < 0.0 {
                                *esc
                            } else {
                                *esc + dt
                            };
                            if *view > 1 && *esc >= 5.0 {
                                *view = match *view {
                                    5 => 1,
                                    v => (v - 1).min(2),
                                };
                                machine.take().into_iter().for_each(|(_, _, on, thread, _, _)| {
                                    on.store(false, std::sync::atomic::Ordering::Release);
                                    thread.unpark();
                                    icons[38] = icons[2].clone();
                                });
                                *modal = false;
                                *picker = false;
                                *esc = -1.0;
                            }
                            machine.iter_mut().for_each(|(pixels, generation, _, _, seen, _)| {
                                Some(generation.load(std::sync::atomic::Ordering::Acquire))
                                    .filter(|g| g != seen)
                                    .into_iter()
                                    .for_each(|g| {
                                        *seen = g;
                                        images.get_mut(&icons[38]).iter_mut().for_each(|image| {
                                            image.data.iter_mut().for_each(|data| {
                                                data.chunks_exact_mut(4).zip(pixels.iter()).for_each(|(rgba, pixel)| {
                                                    rgba.copy_from_slice(&pixel.load(std::sync::atomic::Ordering::Relaxed).to_le_bytes())
                                                });
                                            });
                                        });
                                    });
                            });
                            if *view < 2 && *esc >= 7.0 {
                                commands.write_message(bevy::app::AppExit::Success);
                            }
                            anim.iter_mut().enumerate().for_each(|(id, a)| match bt.iter().find(|b| b.1 == id) {
                                Some(b) => [
                                    ((*hover == Some(id)) as u8 as f32, 0.15),
                                    ((*focus == Some(id)) as u8 as f32, 0.11),
                                    (press.is_some_and(|p| p.0 == id) as u8 as f32, 0.06),
                                    (b.2, 0.15),
                                ]
                                .into_iter()
                                .enumerate()
                                .for_each(|(k, (goal, secs))| {
                                    *moving |= goal != a[k];
                                    a[k] += (goal - a[k]).clamp(-dt / secs, dt / secs);
                                }),
                                None => *a = [0.0; 4],
                            });
                            Some([
                                if *view > 0 && !*hud { (fades[0] + dt / 0.14).min(1.0) } else { 0.0 },
                                if *modal && !*hud { (fades[1] + dt / 0.12).min(1.0) } else { 0.0 },
                                if *picker && !*hud { (fades[2] + dt / 0.10).min(1.0) } else { 0.0 },
                            ])
                            .into_iter()
                            .for_each(|next| {
                                *moving |= next != *fades;
                                *fades = next;
                            });
                            Some(if *moving || *esc != 0.0 || press.is_some() {
                                bevy::winit::UpdateMode::Continuous
                            } else {
                                bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_millis(if machine.is_some() { 16 } else if *view == 5 && xjob.as_ref().is_some_and(|j| j.2) { 50 } else { 60_000 }))
                            })
                            .filter(|mode| winit.focused_mode != *mode)
                            .into_iter()
                            .for_each(|mode| winit.focused_mode = mode);
*moving = false;
}
}
                        },
                    );
                    });
            },
        )
        .run();
    });
}
