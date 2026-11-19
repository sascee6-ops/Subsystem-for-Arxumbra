#![no_std]

pub fn boot(
    pixels: &[core::sync::atomic::AtomicU32],
    width: usize,
    present: impl Fn(),
    halt: impl Fn() -> bool,
) {
    pixels.iter().enumerate().for_each(|(index, pixel)| {
        pixel.store(
            if (index % width / 48 + index / width / 48) % 2 == 0 {
                0xFFFF_FFFF
            } else {
                0xFFE6_E6E6
            },
            core::sync::atomic::Ordering::Relaxed,
        )
    });
    present();
    while halt() {}
}

pub fn cli(
    pixels: &[core::sync::atomic::AtomicU32],
    width: usize,
    present: impl Fn(),
    halt: impl Fn() -> bool,
    key: impl Fn() -> Option<u8>,
    line: &[u8],
    prompt: &[u8],
) {
    pixels
        .iter()
        .for_each(|pixel| pixel.store(0xFF0A_0A0A, core::sync::atomic::Ordering::Relaxed));
    for (unit, state, floor) in [(
        (width / 1280).max(1) * 3,
        core::cell::Cell::new((0_usize, 0_usize, false)),
        core::cell::Cell::new(0_usize),
    )] {
        for fill in [|x: usize, y: usize, w: usize, h: usize, color: u32| {
            (y..y + h).for_each(|row| {
                (x..x + w).for_each(|column| {
                    pixels
                        .get(row * width + column)
                        .iter()
                        .for_each(|pixel| pixel.store(color, core::sync::atomic::Ordering::Relaxed))
                })
            })
        }] {
            for (cols, rows) in [(
                (width.saturating_sub(16 * unit) / (6 * unit)).max(1),
                ((pixels.len() / width).saturating_sub(16 * unit) / (10 * unit)).max(2),
            )] {
                for glyph in [|byte: u8, column: usize, row: usize, color: u32| {
                    (0..5).for_each(|gx| {
                        (0..7).for_each(|gy| {
                            fill(
                                8 * unit + column * 6 * unit + gx * unit,
                                8 * unit + row * 10 * unit + (gy + 1) * unit,
                                unit,
                                unit,
                                if b"\x00\x00\x00\x00\x00\x00\x00\x5F\x00\x00\x00\x07\x00\x07\x00\x14\x7F\x14\x7F\x14\x24\x2A\x7F\x2A\x12\x23\x13\x08\x64\x62\x36\x49\x55\x22\x50\x00\x05\x03\x00\x00\x00\x1C\x22\x41\x00\x00\x41\x22\x1C\x00\x14\x08\x3E\x08\x14\x08\x08\x3E\x08\x08\x00\x50\x30\x00\x00\x08\x08\x08\x08\x08\x00\x60\x60\x00\x00\x20\x10\x08\x04\x02\x3E\x51\x49\x45\x3E\x00\x42\x7F\x40\x00\x42\x61\x51\x49\x46\x21\x41\x45\x4B\x31\x18\x14\x12\x7F\x10\x27\x45\x45\x45\x39\x3C\x4A\x49\x49\x30\x01\x71\x09\x05\x03\x36\x49\x49\x49\x36\x06\x49\x49\x29\x1E\x00\x36\x36\x00\x00\x00\x56\x36\x00\x00\x08\x14\x22\x41\x00\x14\x14\x14\x14\x14\x00\x41\x22\x14\x08\x02\x01\x51\x09\x06\x32\x49\x79\x41\x3E\x7E\x11\x11\x11\x7E\x7F\x49\x49\x49\x36\x3E\x41\x41\x41\x22\x7F\x41\x41\x22\x1C\x7F\x49\x49\x49\x41\x7F\x09\x09\x09\x01\x3E\x41\x49\x49\x7A\x7F\x08\x08\x08\x7F\x00\x41\x7F\x41\x00\x20\x40\x41\x3F\x01\x7F\x08\x14\x22\x41\x7F\x40\x40\x40\x40\x7F\x02\x0C\x02\x7F\x7F\x04\x08\x10\x7F\x3E\x41\x41\x41\x3E\x7F\x09\x09\x09\x06\x3E\x41\x51\x21\x5E\x7F\x09\x19\x29\x46\x46\x49\x49\x49\x31\x01\x01\x7F\x01\x01\x3F\x40\x40\x40\x3F\x1F\x20\x40\x20\x1F\x3F\x40\x38\x40\x3F\x63\x14\x08\x14\x63\x07\x08\x70\x08\x07\x61\x51\x49\x45\x43\x00\x7F\x41\x41\x00\x02\x04\x08\x10\x20\x00\x41\x41\x7F\x00\x04\x02\x01\x02\x04\x40\x40\x40\x40\x40\x00\x01\x02\x04\x00\x20\x54\x54\x54\x78\x7F\x48\x44\x44\x38\x38\x44\x44\x44\x20\x38\x44\x44\x48\x7F\x38\x54\x54\x54\x18\x08\x7E\x09\x01\x02\x0C\x52\x52\x52\x3E\x7F\x08\x04\x04\x78\x00\x44\x7D\x40\x00\x20\x40\x44\x3D\x00\x7F\x10\x28\x44\x00\x00\x41\x7F\x40\x00\x7C\x04\x18\x04\x78\x7C\x08\x04\x04\x78\x38\x44\x44\x44\x38\x7C\x14\x14\x14\x08\x08\x14\x14\x18\x7C\x7C\x08\x04\x04\x08\x48\x54\x54\x54\x20\x04\x3F\x44\x40\x20\x3C\x40\x40\x20\x7C\x1C\x20\x40\x20\x1C\x3C\x40\x30\x40\x3C\x44\x28\x10\x28\x44\x0C\x50\x50\x50\x3C\x44\x64\x54\x4C\x44\x00\x08\x36\x41\x00\x00\x00\x7F\x00\x00\x00\x41\x36\x08\x00\x08\x04\x08\x10\x08".get(usize::from(byte.saturating_sub(32)) * 5 + gx).is_some_and(|bits| *bits >> gy & 1 == 1) {
                                    color
                                } else {
                                    0xFF0A_0A0A
                                },
                            )
                        })
                    })
                }] {
                    for advance in [|row: usize| {
                        if row + 1 == rows {
                            (8 * unit * width..(8 * unit + (rows - 1) * 10 * unit) * width)
                                .for_each(|index| {
                                    pixels
                                        .get(index)
                                        .zip(pixels.get(index + 10 * unit * width))
                                        .iter()
                                        .for_each(|(dst, src)| {
                                            dst.store(
                                                src.load(core::sync::atomic::Ordering::Relaxed),
                                                core::sync::atomic::Ordering::Relaxed,
                                            )
                                        })
                                });
                            fill(
                                8 * unit,
                                8 * unit + (rows - 1) * 10 * unit,
                                cols * 6 * unit,
                                10 * unit,
                                0xFF0A_0A0A,
                            );
                            row
                        } else {
                            row + 1
                        }
                    }] {
                        for step in [|byte: u8| {
                            for (column, row, _) in [state.get()] {
                                match byte {
                                    8 => {
                                        if column > floor.get() {
                                            fill(
                                                8 * unit + (column - 1) * 6 * unit,
                                                8 * unit + row * 10 * unit,
                                                6 * unit,
                                                10 * unit,
                                                0xFF0A_0A0A,
                                            );
                                            state.set((column - 1, row, true))
                                        }
                                    }
                                    10 => {
                                        floor.set(0);
                                        state.set((0, advance(row), true))
                                    }
                                    32..=126 => {
                                        glyph(byte, column, row, 0xFFE6_E6E6);
                                        if column + 1 == cols {
                                            floor.set(0);
                                            state.set((0, advance(row), true))
                                        } else {
                                            state.set((column + 1, row, true))
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }] {
                            for show in [|| {
                                prompt.iter().for_each(|byte| step(*byte));
                                floor.set(state.get().0)
                            }] {
                                Some(line)
                                    .filter(|text| !text.is_empty())
                                    .into_iter()
                                    .for_each(|text| {
                                        text.iter().for_each(|byte| step(*byte));
                                        step(10);
                                    });
                                show();
                                (0_u32..).try_for_each(|_| {
                                    for (column, row, visible) in [state.get()] {
                                        fill(
                                            8 * unit + column * 6 * unit,
                                            8 * unit + row * 10 * unit,
                                            5 * unit,
                                            9 * unit,
                                            0xFF0A_0A0A,
                                        );
                                        for typed in [core::iter::from_fn(|| key())
                                            .map(|byte| {
                                                step(byte);
                                                if byte == 10 {
                                                    show()
                                                }
                                            })
                                            .count()
                                            > 0]
                                        {
                                            for (column, row, _) in [state.get()] {
                                                fill(
                                                    8 * unit + column * 6 * unit,
                                                    8 * unit + row * 10 * unit,
                                                    5 * unit,
                                                    9 * unit,
                                                    if typed || !visible {
                                                        0xFFE6_E6E6
                                                    } else {
                                                        0xFF0A_0A0A
                                                    },
                                                );
                                                state.set((column, row, typed || !visible));
                                            }
                                        }
                                    }
                                    present();
                                    halt().then_some(())
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
