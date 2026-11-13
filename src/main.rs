#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

static DIR: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
    (if cfg!(target_os = "windows") {
        std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap())
    } else if cfg!(target_os = "macos") {
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
            .join("Library/Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME").map_or_else(
            || std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/share"),
            std::path::PathBuf::from,
        )
    })
    .join("Subsystem-for-Arxumbra")
});

static DB: std::sync::LazyLock<redb::Database> = std::sync::LazyLock::new(|| {
    std::fs::create_dir_all(&*DIR).unwrap();
    redb::Database::create(DIR.join("projects.redb")).unwrap()
});

fn main() {
    bevy::app::App::new()
        .add_plugins((
            bevy::app::TaskPoolPlugin::default(),
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
                    title: String::from(if cfg!(target_os = "linux") {
                        "Linux-Subsystem-for-Arxumbra"
                    } else if cfg!(target_os = "macos") {
                        "macOS-Subsystem-for-Arxumbra"
                    } else {
                        "Windows-Subsystem-for-Arxumbra"
                    }),
                    skip_taskbar: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            bevy::asset::AssetPlugin::default(),
            bevy::winit::WinitPlugin::default(),
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
                        ..Default::default()
                    }),
                ),
                ..Default::default()
            },
            bevy::image::ImagePlugin::default(),
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
            bevy::app::Startup,
            |mut commands: bevy::ecs::system::Commands| {
                commands.spawn((bevy::camera::Camera2d, bevy::render::view::Msaa::Off));
            },
        )
        .add_systems(
            bevy::app::Update,
            |window: bevy::ecs::system::Single<
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
             mut kb: bevy::ecs::message::MessageReader<bevy::input::keyboard::KeyboardInput>,
             mut wheel: bevy::ecs::message::MessageReader<bevy::input::mouse::MouseWheel>,
             mut images: bevy::ecs::system::ResMut<bevy::asset::Assets<bevy::image::Image>>,
             mut commands: bevy::ecs::system::Commands,
             mut sprites: bevy::ecs::system::Query<
                (
                    &mut bevy::sprite::Sprite,
                    &mut bevy::transform::components::Transform,
                    &mut bevy::camera::visibility::Visibility,
                ),
                bevy::ecs::query::Without<bevy::sprite::Text2d>,
            >,
             mut texts: bevy::ecs::system::Query<
                (
                    &mut bevy::sprite::Text2d,
                    &mut bevy::text::TextFont,
                    &mut bevy::text::TextColor,
                    &mut bevy::sprite::Anchor,
                    &mut bevy::transform::components::Transform,
                    &mut bevy::camera::visibility::Visibility,
                ),
                bevy::ecs::query::Without<bevy::sprite::Sprite>,
            >,
             mut winit: bevy::ecs::system::ResMut<bevy::winit::WinitSettings>,
             mut ma: bevy::ecs::system::Local<(
                u8,
                bool,
                bool,
                bool,
                f32,
                bool,
                usize,
                Option<bevy::math::Vec2>,
                bevy::math::Rect,
            )>,
             mut fo: bevy::ecs::system::Local<(
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
             mut st: bevy::ecs::system::Local<(
                Vec<(
                    String,
                    Option<(usize, bevy::math::Vec2)>,
                    u8,
                    u8,
                    u64,
                    Option<bevy::math::Vec2>,
                )>,
                usize,
                Vec<(usize, Vec<u8>)>,
            )>,
             mut pt: bevy::ecs::system::Local<(
                Option<usize>,
                Option<usize>,
                Option<(usize, f32)>,
                Vec<[f32; 4]>,
                [f32; 3],
                Vec<(bevy::math::Rect, usize, f32, f32, bool, f32)>,
            )>,
             mut ic: bevy::ecs::system::Local<Vec<bevy::asset::Handle<bevy::image::Image>>>| {
                if ic.is_empty() {
                    ic.extend(
                        [
                            b"\
                              ..........................\
                              ............33............\
                              ...........7ff7...........\
                              .........1cffffc1.........\
                              ........5ffffffff5........\
                              ......1affffffffffa1......\
                              .....4effffffffffffe4.....\
                              ....9ffffffffffffffff9....\
                              ..3dffffffffffffffffffd3..\
                              .178bffffffffffffffffb871.\
                              ....8ffffffffffffffff8....\
                              ....8ffffffffffffffff8....\
                              ....8ffffffffffffffff8....\
                              ....8ffffffffffffffff8....\
                              ....8fffff8....8fffff8....\
                              ....8fffff8....8fffff8....\
                              ....8fffff8....8fffff8....\
                              ....8fffff8....8fffff8....\
                              ....8fffff8....8fffff8....\
                              ....8fffff8....8fffff8....\
                              ....4888884....4888884....\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..afffffffa....afffffffa..\
                              .5fffffffff5..5fffffffff5.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .5fffffffff5..5fffffffff5.\
                              ..afffffffa....afffffffa..\
                              ..........................\
                              ..........................\
                              ..afffffffa....afffffffa..\
                              .5fffffffff5..5fffffffff5.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .8fffffffff8..8fffffffff8.\
                              .5fffffffff5..5fffffffff5.\
                              ..afffffffa....afffffffa..\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ................269cc.....\
                              ..............4bffffc.....\
                              ............2bffffff9.....\
                              ...........3efeadfff6.....\
                              ..........3ffe2.1dff1.....\
                              ........37fffb...9fb......\
                              ....26aefffffe2.2ef4......\
                              ...7ffffffffffebefb.......\
                              ...467affffffffffe2.......\
                              .......5fffffffff3........\
                              .......9ffffffff4.........\
                              ......17fffffff7..........\
                              ......aa5ffffff3..........\
                              .....3ffb795ffe...........\
                              .....affa...afb...........\
                              ....2fb4....7f7...........\
                              ....33......5f3...........\
                              ............48............\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ............11............\
                              ...........8ee9...........\
                              ...........dffe...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ....8ccccccffffcccccc9....\
                              ...1ffffffffffffffffff1...\
                              ...1ffffffffffffffffff1...\
                              ....9ddddddffffdddddda....\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........cffd...........\
                              ...........dffe...........\
                              ...........9ffa...........\
                              ............11............\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              .......cfd4....4dfc.......\
                              ......4fcaea..aeacf4......\
                              ......7e...8bb8...e7......\
                              ......5c....dd....c5......\
                              ......2d...9669...d2......\
                              .......e..58..85..e.......\
                              .......941b.44.b149.......\
                              .......2aa3dffd3aa2.......\
                              ........b94ffff49b........\
                              ........b94ffff49b........\
                              .......2aa3dffd3aa2.......\
                              .......941b.44.b149.......\
                              .......e..58..85..e.......\
                              ......2d...9669...d2......\
                              ......5c....dd....c5......\
                              ......7e...8bb8...e7......\
                              ......4fcaea..aeacf4......\
                              .......cfd4....4dfc.......\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........799aa997.........\
                              .........ffffffff1........\
                              .........4affffa4.........\
                              ..........6ffff6..........\
                              ..........7ffff7..........\
                              ..........7ffff7..........\
                              ..........7ffff7..........\
                              ..........6ffff6..........\
                              ..........affffa..........\
                              .........5ffffff5.........\
                              ........1effffffe1........\
                              ........cffffffffc........\
                              .......188888888881.......\
                              .......1..........1.......\
                              ......bffffffffffffb......\
                              .....7ffffeffd5bffff7.....\
                              ....3ffff618fb.8fffff3....\
                              ....cffff..3ffdfeffffc....\
                              ....effffa5cfff5.dfffe....\
                              ....dffffffffff4.cfffd....\
                              ....3effffffffffffffe3....\
                              .....2444444444444442.....\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........555555..........\
                              .........affffffb.........\
                              .........ff7557ff1........\
                              ........1fd....cf1........\
                              .........fd....cf.........\
                              ....36666fe6666ef66664....\
                              ...7ffffffffffffffffff8...\
                              ...efffffffffffffffffff1..\
                              ..1efffffffffffffffffff1..\
                              ..1ffffffffffffffffffff1..\
                              ..1efeffffffffffffffefe1..\
                              ..........1efff2..........\
                              ...abbbbbbbffffcbbbbbbb1..\
                              ..1ffffffffffffffffffff1..\
                              ..1efffffffffffffffffff1..\
                              ..1efffffffffffffffffff1..\
                              ..1ffffffffffffffffffff1..\
                              ...dffffffffffffffffffe...\
                              ...4dffffffffffffffffd4...\
                              .....1111111111111111.....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ............11............\
                              ...........beeb...........\
                              ...........dffd...........\
                              .....3b2...effe...2b3.....\
                              ....3efe428ffff824efe3....\
                              ....cffffffffffffffffc....\
                              ....2ffffffffffffffff2....\
                              .....4ffffffffffffff4.....\
                              .....1ffffffbbffffff1.....\
                              .....9ffffc2..2cffff9.....\
                              ..bddfffff1....1fffffedb1.\
                              .1fffffffb......bfffffff1.\
                              .1fffffffb......bfffffff1.\
                              ..bddfffff1....1fffffedb1.\
                              .....9ffffc2..2cffff9.....\
                              .....1ffffffbbffffff1.....\
                              .....4ffffffffffffff4.....\
                              ....2ffffffffffffffff2....\
                              ....cffffffffffffffffc....\
                              ....3efe428ffff824efe3....\
                              .....3b2...effe...2b3.....\
                              ...........dffd...........\
                              ...........cffc...........\
                              ...........1111...........\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              .........52.44.151........\
                              ........4f9.ef.7f5........\
                              ........4f8.ee.7f5........\
                              ........4f8.ef.7f5........\
                              .......6bfd9ff9cfc7.......\
                              ......6ffffffffffff7......\
                              ...444cffffffffffffc4441..\
                              ..6ffffffa554455affffff7..\
                              ..2888dff4......3ffd8882..\
                              ......9ff5.dffe.4ffa......\
                              ..4eeefff4.ffff.3fffeee5..\
                              ..4eeffff4.ffff.3ffffef5..\
                              ......9ff5.effe.4ffa......\
                              ..1777cff4......3ffd7772..\
                              ..6ffffffa4433449ffffff7..\
                              ..1555cffffffffffffd5551..\
                              ......6ffffffffffff7......\
                              .......7cfdaffadfc7.......\
                              ........4f8.ef.7f5........\
                              ........4f8.ee.7f5........\
                              ........4f9.ef.7f5........\
                              ........162.55.261........\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...5bddddcddcccccccccb5...\
                              ..4ff8be7f9afffffffffff5..\
                              ..9ff15b.d24fffffffffffa..\
                              ..affffffffffffffffffffa..\
                              ..9f933333333333333339fa..\
                              ..9f2................1fa..\
                              ..9f3.6e5............2fb..\
                              ..9f3.6ff7...........2fb..\
                              ..9f3..5ff9..........2fb..\
                              ..9f3...3dfc.........2fb..\
                              ..9f3...2dfc.........2fb..\
                              ..9f3..4ffa..........2fb..\
                              ..9f3.6ff8..2cdddd4..2fb..\
                              ..9f3.6e5...2deeee5..2fb..\
                              ..9f2................1fb..\
                              ..9f7................6fa..\
                              ..4ffeeeeeeeeeeeeeeeeff5..\
                              ...6cddddddddddddddddc6...\
                              ..........................\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....233333333333333332....\
                              ..1bffffffffffffffffffc1..\
                              ..7ff36b1d35fffffffffff9..\
                              ..aff69d5e78fffffffffffb..\
                              ..9ffeededeedddddddddffa..\
                              ..9f5................4fa..\
                              ..9f2................1fb..\
                              ..9f3................2fb..\
                              ..9f3.........3......2fb..\
                              ..9f3.........7c.....2fb..\
                              ..9f3.........6fb....2fb..\
                              ..9f3.........7ffc...2fb..\
                              ..9f3.........7fffc1.2fa..\
                              ..9f2.........7ffffd11fb..\
                              ..9f722222222.6fffffd15a..\
                              ..4ffffffffff45ffffffe2...\
                              ...4aaaaaaaab26ffffdbca...\
                              ..............7ffdfc......\
                              ..............7f71ff4.....\
                              ..............78..8fc.....\
                              ..................1ff3....\
                              ...................53.....\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ....5563..........2655....\
                              ...5fffd..........cfff6...\
                              ...6fffd..........cfff7...\
                              ...6ffffbbbbbbbbbbefff7...\
                              ...3ddffffffffffffffdd3...\
                              ......bf1..1111..1fc......\
                              ......bf..........fc......\
                              ......bf...4bb4...fc......\
                              ......bf1.4ffff4..fc......\
                              ......bf1.bffffb..fc......\
                              ......bf1.bffffb..fc......\
                              ......bf1.4ffff4..fc......\
                              ......bf...4bb4...fc......\
                              ......bf..........fc......\
                              ......bf1.........fc......\
                              ...2ccefffffffffffffcc3...\
                              ...6ffffccccccccccffff7...\
                              ...6fffd..........cfff7...\
                              ...5fffd..........cfff6...\
                              ....6673..........3666....\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ............23............\
                              ..........18ff91..........\
                              .........6effffe6.........\
                              .......4cffffffffc4.......\
                              .....2affffffffffffa2.....\
                              ....7ffffffffffffffff8....\
                              ....2affffffffffffffb3....\
                              ...11.4cffffffffffd4.11...\
                              ...4f8..6effffffe7..8f4...\
                              ...4ffd5..9ffff91.5dff4...\
                              ...4ffffc3.2bc3.3cffff4...\
                              ...4ffffffa1..1affffff4...\
                              ...4ffffffff11ffffffff4...\
                              ...4ffffffff22ffffffff4...\
                              ...4ffffffff11ffffffff4...\
                              ...4ffffffff11ffffffff4...\
                              ...2cfffffff11fffffffc2...\
                              .....6efffff11fffffe6.....\
                              ......19ffff11ffff91......\
                              ........3bff11ffb3........\
                              ..........5d22d5..........\
                              ..........................\
                              ..........................\
                              .........................."
                                .as_slice(),
                            b"\
                              ..........................\
                              ..........................\
                              ..........................\
                              ..........................\
                              ...9cccccca1..............\
                              ..5ffffffff6..............\
                              ..6ffffffff6..............\
                              ..5ffffffffc9a999999994...\
                              ..5ffffffffffffffffffff3..\
                              ..6ffffffffffffffffffff7..\
                              ..3888888888888888888883..\
                              ..1323333333333333333231..\
                              ..6ffffffffffffffffffff7..\
                              ..5ffffffffffffffffffff6..\
                              ..6ffffffffffffffffffff7..\
                              ..6ffffffffffffffffffff7..\
                              ..6ffffffffffffffffffff7..\
                              ..6ffffffffffffffffffff7..\
                              ..5ffffffffffffffffffff6..\
                              ..6ffffffffffffffffffff7..\
                              ..4ffffffffffffffffffff4..\
                              ...7dddddddddddddddddd8...\
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
                              ...2cffffffffffffffffc3...\
                              ...cfecdccccccccccdcdfd...\
                              ...fe................df1..\
                              ...fc................bf1..\
                              ...fd..........274...cf1..\
                              ...fc.........1eff4..bf1..\
                              ...fc.........5fff9..bf1..\
                              ...fc.........1dfe3..bf1..\
                              ...fc.....6....142...bf1..\
                              ...fc....4f8.........bf1..\
                              ...fc...1eff4..41....bf1..\
                              ...fc...afffe.4fc....bf1..\
                              ...fc..5fffffceffa...bf1..\
                              ...fd..effffffffff8..cf1..\
                              ...fc.bffffffffffff8.bf1..\
                              ...fd166666666666666.cf1..\
                              ...cfdaaaaaaaaaaaaaadfd...\
                              ...3cffffffffffffffffd3...\
                              .....1111111111111111.....\
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
                    ic.push(bevy::asset::Handle::default());
                    st.0 = redb::ReadableDatabase::begin_read(&*DB)
                        .unwrap()
                        .open_table(redb::TableDefinition::<&str, &[u8]>::new("fs"))
                        .into_iter()
                        .flat_map(|table| {
                            table
                                .range("/projects/".."/projects0")
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
                                            .get(format!("{}block", &key.value()[..27]).as_str())
                                            .unwrap()
                                            .map(|at| {
                                                bevy::math::Vec2::new(
                                                    f32::from_le_bytes(at.value()[..4].try_into().unwrap()),
                                                    f32::from_le_bytes(at.value()[4..8].try_into().unwrap()),
                                                )
                                            }),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .enumerate()
                        .map(|(row, (name, png, kind, dim, id, at))| {
                            st.2.extend(png.map(|bytes| (row, bytes)));
                            (name, None, kind, dim, id, at)
                        })
                        .collect();
                    commands.spawn_batch((0..208).map(|_| {
                        (
                            bevy::sprite::Sprite {
                                custom_size: Some(bevy::math::Vec2::ONE),
                                ..Default::default()
                            },
                            bevy::transform::components::Transform::default(),
                            bevy::camera::visibility::Visibility::Hidden,
                        )
                    }));
                    commands.spawn_batch((0..72).map(|_| {
                        (
                            bevy::sprite::Text2d::new(""),
                            bevy::text::TextFont::default(),
                            bevy::text::TextColor::default(),
                            bevy::sprite::Anchor::CENTER_LEFT,
                            bevy::transform::components::Transform::default(),
                            bevy::camera::visibility::Visibility::Hidden,
                        )
                    }));
                    ma.5 = true;
                }
                pt.3.resize(34 + 2 * st.0.len(), [0.0; 4]);
                Some(((&mut *ma, &mut *fo, &mut *st, &mut *pt, &mut *ic), window.width(), window.height()))
                    .map(|(l, w, h)| (l, w, h, (w * 0.22).clamp(200.0, 280.0).min(w * 0.5)))
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
                            (cols, (((h - 136.0) / 160.0).max(1.0) as usize).min(16 / cols)),
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
                                (view, hud, modal, picker, esc, moving, ed, drag, brect),
                                (name, icon, kind, dim, dir, entries, pscroll, dirty, wacc, pending),
                                (store, lscroll, jobs),
                                (hover, focus, press, anim, fades, bt),
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
                            (cols, rows),
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
                                if e.state.is_pressed() && *modal && !*picker && !*hud {
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
                                .into_iter()
                                .for_each(|(mut sit, mut tit)| {
                                    Some((
                                        |r: bevy::math::Rect, z: f32, c: bevy::color::Color, i: usize| {
                                            sit.next().into_iter().for_each(|(mut s, mut t, mut v)| {
                                                if s.color != c {
                                                    s.color = c;
                                                }
                                                if s.image != icons[i] {
                                                    s.image = icons[i].clone();
                                                }
                                                if s.custom_size != Some(r.size()) {
                                                    s.custom_size = Some(r.size());
                                                }
                                                Some(bevy::math::Vec3::new(r.center().x - w / 2.0, h / 2.0 - r.center().y, z))
                                                    .filter(|p| t.translation != *p)
                                                    .into_iter()
                                                    .for_each(|p| t.translation = p);
                                                if *v != bevy::camera::visibility::Visibility::Inherited {
                                                    *v = bevy::camera::visibility::Visibility::Inherited;
                                                }
                                            });
                                        },
                                        |x: f32, y: f32, an: usize, label: &str, size: f32, z: f32, c: bevy::color::Color| {
                                            tit.next().into_iter().for_each(|(mut tx, mut tf, mut tc, mut ta, mut t, mut v)| {
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
                                                .filter(|a| *ta != *a)
                                                .into_iter()
                                                .for_each(|a| *ta = a);
                                                Some(bevy::math::Vec3::new(x - w / 2.0, h / 2.0 - y, z))
                                                    .filter(|p| t.translation != *p)
                                                    .into_iter()
                                                    .for_each(|p| t.translation = p);
                                                if *v != bevy::camera::visibility::Visibility::Inherited {
                                                    *v = bevy::camera::visibility::Visibility::Inherited;
                                                }
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
                                    .into_iter()
                                    .for_each(|(mut spr, mut txt, gr, rc, cs)| {
                                        if !*hud {
                                            spr(rc(0.0, 0.0, side, h), 2.0, gr(0.071, 1.0), 15);
                                            spr(rc(side - 1.0, 0.0, side, h), 3.0, gr(0.149, 1.0), 15);
                                            spr(rc(0.0, 60.0, side - 1.0, 61.0), 3.0, gr(0.149, 1.0), 15);
                                            spr(cs(30.0, 32.0, 18.0, 18.0), 6.0, gr(0.871, 1.0), if *view == 2 { 5 } else { 4 });
                                            txt(
                                                48.0,
                                                22.0,
                                                1,
                                                if *view == 2 { "Laboratory" } else { window.title.as_str() },
                                                if *view == 2 { 15.0 } else { ((side - 56.0) / (0.6 * window.title.len() as f32)).clamp(9.0, 15.0) },
                                                6.0,
                                                gr(0.871, 1.0),
                                            );
                                            if *view < 2 {
                                                for (k, (r, label)) in [
                                                    (rc(20.0, 78.0, (side - 20.0).max(21.0), 130.0), "Home"),
                                                    (rc(20.0, 138.0, (side - 20.0).max(21.0), 190.0), "Extension Registry"),
                                                ]
                                                .into_iter()
                                                .enumerate()
                                                {
                                                    bt.push((r, k, (*view as usize == k) as u8 as f32, 5.0, !*modal && !*picker, 1.0));
                                                    spr(cs(48.0, r.center().y, 20.0, 20.0 * 22.0 / 26.0), 6.0, gr(0.871 - 0.804 * anim[k][3], 1.0), k);
                                                    txt(68.0, r.center().y, 0, label, 15.0, 6.5, gr(0.871 - 0.804 * anim[k][3], 1.0));
                                                }
                                            } else if *view == 2 {
                                                bt.push((rc(20.0, 78.0, (side - 20.0).max(21.0), 130.0), 3, 0.0, 5.0, !*modal && !*picker, 1.0));
                                                spr(cs(48.0, 104.0, 18.0, 18.0), 6.0, gr(0.871, 1.0), 3);
                                                txt(68.0, 104.0, 0, "Create project", 15.0, 6.5, gr(0.871, 1.0));
                                            } else {
                                                spr(rc(20.0, 78.0, side - 20.0, 130.0), 3.5, gr(0.149, 1.0), 15);
                                                spr(rc(21.0, 79.0, side - 21.0, 129.0), 3.6, gr(0.071, 1.0), 15);
                                            }
                                            if *view > 0 {
                                                for a in [fades[0]] {
                                                    spr(rc(side + 20.0, 24.0, (w - 24.0).max(side + 60.0), (h - 24.0).max(48.0)), 1.0, gr(0.169, a), 15);
                                                    spr(rc(side + 21.0, 25.0, (w - 25.0).max(side + 59.0), (h - 25.0).max(47.0)), 1.5, gr(0.078, a), 15);
                                                    if *view == 1 {
                                                        for cd in [rc(side + 44.0, 48.0, xr, 192.0)] {
                                                            spr(cd, 1.6, gr(0.169, a), 15);
                                                            spr(cd.inflate(-1.0), 1.7, gr(0.106, a), 15);
                                                            spr(rc(cd.min.x + 20.0, cd.min.y + 64.0, cd.max.x - 20.0, cd.min.y + 65.0), 1.8, gr(0.169, a), 15);
                                                            spr(cs(cd.min.x + 35.0, cd.min.y + 32.0, 26.0, 26.0), 6.0, gr(0.871, a), 5);
                                                            txt(cd.min.x + 62.0, cd.min.y + 32.0, 0, "Laboratory", 18.0, 6.5, gr(0.871, a));
                                                            for lr in [rc(cd.max.x - 128.0, cd.max.y - 52.0, cd.max.x - 16.0, cd.max.y - 16.0)] {
                                                                bt.push((lr, 2, 0.0, 5.0, !*modal && !*picker, a));
                                                                spr(cs(lr.min.x + 22.0, lr.center().y, 18.0, 18.0), 6.0, gr(0.871, a), 2);
                                                                txt(lr.min.x + 40.0, lr.center().y, 0, "Launch", 14.0, 6.5, gr(0.871, a));
                                                            }
                                                        }
                                                    } else {
                                                        spr(cs(side + 55.0, 56.0, 26.0, 26.0), 6.0, gr(0.871, a), if *view == 2 { 6 } else { 8 });
                                                        txt(
                                                            side + 82.0,
                                                            56.0,
                                                            0,
                                                            if *view == 2 { "Workspace" } else { store[*ed].0.as_str() },
                                                            18.0,
                                                            6.5,
                                                            gr(0.871, a),
                                                        );
                                                        spr(rc(side + 40.0, 88.0, (w - 44.0).max(side + 41.0), 89.0), 1.8, gr(0.169, a), 15);
                                                        if *view == 2 {
                                                            for (i, pr) in store.iter().enumerate().skip(*lscroll * cols).take(rows * cols) {
                                                                for pc in [(side + 40.0 + (i % cols) as f32 * 396.0, 108.0 + (i / cols - *lscroll) as f32 * 160.0)]
                                                                    .map(|(x, y)| rc(x, y, x + 380.0, y + 144.0))
                                                                {
                                                                    spr(pc, 2.0, gr(0.169, a), 15);
                                                                    spr(pc.inflate(-1.0), 2.1, gr(0.106, a), 15);
                                                                    spr(rc(pc.min.x + 20.0, pc.min.y + 64.0, pc.max.x - 20.0, pc.min.y + 65.0), 2.2, gr(0.169, a), 15);
                                                                    match pr.1 {
                                                                        Some((index, size)) => spr(
                                                                            cs(pc.min.x + 35.0, pc.min.y + 32.0, size.x * 26.0, size.y * 26.0),
                                                                            6.0,
                                                                            gr(1.0, a),
                                                                            index,
                                                                        ),
                                                                        None => spr(cs(pc.min.x + 35.0, pc.min.y + 32.0, 22.0, 22.0), 6.0, gr(0.871, a), 4),
                                                                    }
                                                                    txt(pc.min.x + 62.0, pc.min.y + 32.0, 0, pr.0.as_str(), 18.0, 6.5, gr(0.871, a));
                                                                    for (k, lr) in [
                                                                        rc(pc.max.x - 172.0, pc.max.y - 52.0, pc.max.x - 136.0, pc.max.y - 16.0),
                                                                        rc(pc.max.x - 128.0, pc.max.y - 52.0, pc.max.x - 16.0, pc.max.y - 16.0),
                                                                    ]
                                                                    .into_iter()
                                                                    .enumerate()
                                                                    {
                                                                        for ready in [k == 0 || pr.2 != 1 || pr.5.is_some()] {
                                                                            bt.push((lr, 32 + 2 * i + k, 0.0, 5.0, !*modal && !*picker && ready, a));
                                                                            spr(
                                                                                cs(if k == 0 { lr.center().x } else { lr.min.x + 22.0 }, lr.center().y, 18.0, 18.0),
                                                                                6.0,
                                                                                gr(0.871, a * (0.35 + 0.65 * ready as u8 as f32)),
                                                                                if k == 0 { 7 } else { 2 },
                                                                            );
                                                                            if k == 1 {
                                                                                txt(
                                                                                    lr.min.x + 40.0,
                                                                                    lr.center().y,
                                                                                    0,
                                                                                    "Launch",
                                                                                    14.0,
                                                                                    6.5,
                                                                                    gr(0.871, a * (0.35 + 0.65 * ready as u8 as f32)),
                                                                                );
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            spr(cv, 1.6, gr(0.125, a), 15);
                                                            spr(cv.inflate(-1.0), 1.7, gr(0.047, a), 15);
                                                            if store[*ed].5.is_none() {
                                                                txt(cv.center().x, cv.center().y, 2, "Drag the Boot image block here", 14.0, 6.5, gr(0.337, a));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            if *view == 3 {
                                                *brect = match (*drag, store[*ed].5, window.cursor_position()) {
                                                    (Some(off), _, Some(c)) => rc(c.x - off.x, c.y - off.y, c.x - off.x + side - 40.0, c.y - off.y + 52.0),
                                                    (Some(_), _, None) => *brect,
                                                    (None, Some(at), _) => {
                                                        rc(cv.min.x + at.x, cv.min.y + at.y, cv.min.x + at.x + side - 40.0, cv.min.y + at.y + 52.0)
                                                    }
                                                    (None, None, _) => rc(20.0, 78.0, side - 20.0, 130.0),
                                                };
                                                for hv in [window.cursor_position().is_some_and(|c| brect.contains(c)) as u8 as f32] {
                                                    spr(*brect, 8.0, gr(0.169 + 0.15 * hv, fades[0]), 15);
                                                    spr(brect.inflate(-1.0), 8.1, gr(0.106 + 0.05 * hv, fades[0]), 15);
                                                    spr(cs(brect.min.x + 26.0, brect.center().y, 22.0, 22.0), 8.2, gr(0.871, fades[0]), 8);
                                                    txt(brect.min.x + 54.0, brect.center().y, 0, "Boot image", 15.0, 8.3, gr(0.871, fades[0]));
                                                }
                                            }
                                            if *modal {
                                                for a in [fades[1]] {
                                                    spr(rc(0.0, 0.0, w, h), 20.0, bevy::color::Color::srgba(0.0, 0.0, 0.0, 0.6 * a), 15);
                                                    spr(mrect, 21.0, gr(0.169, a), 15);
                                                    spr(mrect.inflate(-1.0), 22.0, gr(0.078, a), 15);
                                                    spr(cs(mrect.min.x + 35.0, mrect.min.y + 32.0, 26.0, 26.0), 23.0, gr(0.871, a), 3);
                                                    txt(mrect.min.x + 62.0, mrect.min.y + 32.0, 0, "Create project", 18.0, 23.0, gr(0.871, a));
                                                    for dy in [64.0, 416.0] {
                                                        spr(rc(mrect.min.x + 20.0, mrect.min.y + dy, mrect.max.x - 20.0, mrect.min.y + dy + 1.0), 23.0, gr(0.169, a), 15);
                                                    }
                                                    for (dy, label) in [(80.0, "Icon"), (176.0, "Name"), (244.0, "Type")] {
                                                        txt(mrect.min.x + 24.0, mrect.min.y + dy, 1, label, 12.0, 23.0, gr(0.459, a));
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
                                                            None => spr(cs(ir.center().x, ir.center().y, 28.0, 28.0), 25.0, gr(0.459, a), 14),
                                                        }
                                                        txt(
                                                            ir.max.x + 16.0,
                                                            ir.center().y,
                                                            0,
                                                            if icon.is_some() { "PNG selected - click to change" } else { "Click to choose a PNG file" },
                                                            13.0,
                                                            23.0,
                                                            gr(0.459, a),
                                                        );
                                                    }
                                                    for fr in [rc(mrect.min.x + 24.0, mrect.min.y + 196.0, mrect.max.x - 24.0, mrect.min.y + 232.0)] {
                                                        spr(fr, 23.0, gr(0.337, a), 15);
                                                        spr(fr.inflate(-1.0), 23.2, gr(0.047, a), 15);
                                                        txt(
                                                            fr.min.x + 12.0,
                                                            fr.center().y,
                                                            0,
                                                            if name.is_empty() { "Project name" } else { name.as_str() },
                                                            15.0,
                                                            24.5,
                                                            gr(if name.is_empty() { 0.337 } else { 0.871 }, a),
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
                                                                gr(0.871, a),
                                                                15,
                                                            );
                                                        }
                                                    }
                                                    for (k, (index, label)) in [(8, "Operating System"), (9, "Command Line Interface"), (10, "Graphical User Interface")]
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
                                                            spr(cs(tr.min.x + 26.0, tr.center().y, 22.0, 22.0), 25.0, gr(0.871 - 0.804 * anim[5 + k][3], a), index);
                                                            txt(tr.min.x + 54.0, tr.center().y, 0, label, 14.0, 25.5, gr(0.871 - 0.804 * anim[5 + k][3], a));
                                                        }
                                                    }
                                                    if *kind == 3 {
                                                        for (k, (index, label)) in [(11, "2D"), (12, "3D")].into_iter().enumerate() {
                                                            for dr in [rc(
                                                                mrect.min.x + 24.0 + k as f32 * 132.0,
                                                                mrect.min.y + 432.0,
                                                                mrect.min.x + 144.0 + k as f32 * 132.0,
                                                                mrect.min.y + 468.0,
                                                            )] {
                                                                bt.push((dr, 8 + k, (*dim as usize == k + 1) as u8 as f32, 24.0, !*picker, a));
                                                                spr(cs(dr.min.x + 24.0, dr.center().y, 18.0, 18.0), 25.0, gr(0.871 - 0.804 * anim[8 + k][3], a), index);
                                                                txt(dr.min.x + 44.0, dr.center().y, 0, label, 14.0, 25.5, gr(0.871 - 0.804 * anim[8 + k][3], a));
                                                            }
                                                        }
                                                    }
                                                    for (sr, ok) in [(
                                                        rc(mrect.max.x - 136.0, mrect.min.y + 432.0, mrect.max.x - 24.0, mrect.min.y + 468.0),
                                                        !name.is_empty() && *kind > 0 && (*kind < 3 || *dim > 0),
                                                    )] {
                                                        bt.push((sr, 10, 0.0, 24.0, ok && !*picker, a * if ok { 1.0 } else { 0.4 }));
                                                        txt(sr.center().x, sr.center().y, 2, "SAVE", 14.0, 25.5, gr(0.871, a * if ok { 1.0 } else { 0.4 }));
                                                    }
                                                }
                                            }
                                            if *picker {
                                                for a in [fades[2]] {
                                                    spr(rc(0.0, 0.0, w, h), 30.0, bevy::color::Color::srgba(0.0, 0.0, 0.0, 0.5 * a), 15);
                                                    spr(prect, 31.0, gr(0.169, a), 15);
                                                    spr(prect.inflate(-1.0), 32.0, gr(0.078, a), 15);
                                                    spr(cs(prect.min.x + 35.0, prect.min.y + 32.0, 26.0, 26.0), 33.0, gr(0.871, a), 13);
                                                    txt(prect.min.x + 62.0, prect.min.y + 32.0, 0, "Choose PNG icon", 18.0, 33.0, gr(0.871, a));
                                                    spr(rc(prect.min.x + 20.0, prect.min.y + 64.0, prect.max.x - 20.0, prect.min.y + 65.0), 33.0, gr(0.169, a), 15);
                                                    for path in [dir.to_string_lossy()] {
                                                        txt(
                                                            prect.min.x + 24.0,
                                                            prect.min.y + 76.0,
                                                            1,
                                                            &path[path.char_indices().rev().nth(63).map_or(0, |c| c.0)..],
                                                            12.0,
                                                            33.0,
                                                            gr(0.459, a),
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
                                                                spr(cs(rr.min.x + 22.0, rr.center().y, 16.0, 16.0), 34.0, gr(0.871, a), if folder { 13 } else { 14 });
                                                                txt(
                                                                    rr.min.x + 40.0,
                                                                    rr.center().y,
                                                                    0,
                                                                    &label[..label.char_indices().nth(52).map_or(label.len(), |c| c.0)],
                                                                    14.0,
                                                                    34.5,
                                                                    gr(0.871, a),
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            for (r, id, _, z, _, al) in bt.iter() {
                                                for ([hv, fc, pr, sl], ga) in [(
                                                    anim[*id],
                                                    if (16..28).contains(id) { *al * anim[*id][0].max(anim[*id][1]) } else { *al },
                                                )] {
                                                    if fc > 0.0 {
                                                        spr(r.inflate(2.0), z - 1.0, gr(0.459, fc * al), 15);
                                                    }
                                                    if ga > 0.0 {
                                                        spr(
                                                            *r,
                                                            *z,
                                                            gr((0.125 + 0.8 * sl + (0.063 + 0.012 * sl) * hv) * (1.0 - pr) + (0.235 + 0.615 * sl) * pr, ga),
                                                            15,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    });
                                    sit.for_each(|(_, _, mut v)| {
                                        if *v != bevy::camera::visibility::Visibility::Hidden {
                                            *v = bevy::camera::visibility::Visibility::Hidden;
                                        }
                                    });
                                    tit.for_each(|(.., mut v)| {
                                        if *v != bevy::camera::visibility::Visibility::Hidden {
                                            *v = bevy::camera::visibility::Visibility::Hidden;
                                        }
                                    });
                                });
                            *hover = window
                                .cursor_position()
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
                                                && !(*modal && !*picker))
                                    })),
                            )
                            .into_iter()
                            .for_each(|act| {
                                *press = press.filter(|_| !mup);
                                *moving |= act.is_some() || mdn || mup || keys.get_just_pressed().len() > 0;
                                if keys.just_pressed(bevy::input::keyboard::KeyCode::Escape)
                                    || (mdn
                                        && window
                                            .cursor_position()
                                            .is_some_and(|c| !(if *picker { prect } else { mrect }).contains(c)))
                                {
                                    *modal &= *picker;
                                    *picker = false;
                                }
                                match act {
                                    Some(i @ 0..=2) => *view = i as u8,
                                    Some(3) => *modal = true,
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
                                            Some(DB.begin_write().unwrap()).into_iter().for_each(|tx| {
                                                Some(tx.open_table(redb::TableDefinition::<&str, &[u8]>::new("fs")).unwrap())
                                                    .into_iter()
                                                    .for_each(|mut table| {
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
                                            store.push((std::mem::take(name), icon.take(), *kind, *dim, id, None));
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
                                    Some(i @ 32..) => {
                                        for (project, gear) in [((i - 32) / 2, i % 2 == 0)] {
                                            if store[project].2 == 1 {
                                                if gear {
                                                    *ed = project;
                                                    *view = 3;
                                                } else if store[project].5.is_some() {
                                                    for (kernel, image) in
                                                        [(DIR.join("kernel"), DIR.join(format!("{:016x}.img", store[project].4)))]
                                                    {
                                                        std::fs::write(&kernel, include_bytes!(env!("CARGO_BIN_FILE_KERNEL_kernel"))).unwrap();
                                                        bootloader::DiskImageBuilder::new(kernel).create_bios_image(&image).unwrap();
                                                        std::process::Command::new("qemu-system-x86_64")
                                                            .args([
                                                                "-no-reboot",
                                                                "-name",
                                                                store[project].0.replace(',', ",,").as_str(),
                                                                "-drive",
                                                                format!("format=raw,snapshot=on,file={}", image.display()).as_str(),
                                                            ])
                                                            .spawn()
                                                            .ok();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            });
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
                            drag.filter(|_| mup).into_iter().for_each(|off| {
                                window.cursor_position().into_iter().for_each(|c| {
                                    store[*ed].5 = Some((c - off - cv.min).clamp(
                                        bevy::math::Vec2::ZERO,
                                        (cv.size() - brect.size()).max(bevy::math::Vec2::ZERO),
                                    ))
                                    .filter(|_| cv.contains(c));
                                    for key in [format!("/projects/{:016x}/block", store[*ed].4)] {
                                        Some(DB.begin_write().unwrap()).into_iter().for_each(|tx| {
                                            Some(tx.open_table(redb::TableDefinition::<&str, &[u8]>::new("fs")).unwrap())
                                                .into_iter()
                                                .for_each(|mut table| {
                                                    match store[*ed].5 {
                                                        Some(at) => table
                                                            .insert(key.as_str(), [at.x.to_le_bytes(), at.y.to_le_bytes()].concat().as_slice())
                                                            .unwrap(),
                                                        None => table.remove(key.as_str()).unwrap(),
                                                    };
                                                });
                                            tx.commit().unwrap();
                                        });
                                    }
                                });
                            });
                            *drag = if mdn && *view == 3 {
                                window.cursor_position().filter(|c| brect.contains(*c)).map(|c| c - brect.min)
                            } else if mup {
                                None
                            } else {
                                *drag
                            };
                            *moving |= drag.is_some();
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
                                *view -= 1;
                                *modal = false;
                                *picker = false;
                                *esc = -1.0;
                            }
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
                                bevy::winit::UpdateMode::reactive_low_power(std::time::Duration::from_secs(60))
                            })
                            .filter(|mode| winit.focused_mode != *mode)
                            .into_iter()
                            .for_each(|mode| winit.focused_mode = mode);
                            *moving = false;
                        },
                    );
            },
        )
        .run();
}
