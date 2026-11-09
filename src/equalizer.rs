pub const FREQUENCIES: [f32; 10] = [
    60., 170., 310., 600., 1000., 3000., 6000., 12000., 14000., 16000.,
];
pub struct Preset {
    pub name: &'static str,
    pub gains: [f32; 10],
}
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "По умолчанию",
        gains: [0.; 10],
    },
    Preset {
        name: "Классическая музыка",
        gains: [0., 0., 0., 0., 0., 0., -4., -4., -4., -5.],
    },
    Preset {
        name: "Клубная музыка",
        gains: [0., 0., 5., 4., 4., 4., 2., 0., 0., 0.],
    },
    Preset {
        name: "Танцевальная музыка",
        gains: [6., 4., 2., 0., 0., -3., -4., -4., 0., 0.],
    },
    Preset {
        name: "Усиление НЧ",
        gains: [7., 6., 5., 4., 2., 0., 0., 0., 0., 0.],
    },
    Preset {
        name: "Усиление НЧ и ВЧ",
        gains: [6., 5., 2., 0., -2., 0., 2., 4., 5., 6.],
    },
    Preset {
        name: "Усиление ВЧ",
        gains: [0., 0., 0., 0., 0., 2., 4., 6., 7., 8.],
    },
    Preset {
        name: "Колонки ноутбука",
        gains: [3., 7., 3., -2., -1., 1., 3., 6., 8., 9.],
    },
    Preset {
        name: "Большой зал",
        gains: [7., 7., 4., 4., 0., -3., -3., -3., 0., 0.],
    },
    Preset {
        name: "Концерт",
        gains: [-4., 0., 3., 4., 4., 4., 3., 2., 2., 2.],
    },
    Preset {
        name: "Вечеринка",
        gains: [4., 4., 0., 0., 0., 0., 0., 0., 4., 4.],
    },
    Preset {
        name: "Поп",
        gains: [-1., 3., 4., 5., 3., 0., -1., -1., -1., -1.],
    },
    Preset {
        name: "Регги",
        gains: [0., 0., 0., -3., 0., 4., 4., 0., 0., 0.],
    },
    Preset {
        name: "Рок",
        gains: [5., 3., -3., -5., -2., 3., 5., 6., 6., 6.],
    },
    Preset {
        name: "Ска",
        gains: [-2., -3., -3., 0., 3., 4., 5., 4., 3., 4.],
    },
    Preset {
        name: "Мягкое звучание",
        gains: [3., 1., 0., -2., 0., 3., 5., 6., 7., 8.],
    },
    Preset {
        name: "Софт-рок",
        gains: [3., 3., 1., 0., -3., -4., -2., 0., 2., 5.],
    },
    Preset {
        name: "Техно",
        gains: [5., 4., 0., -4., -3., 0., 5., 6., 6., 5.],
    },
];
#[cfg(test)]
mod tests {
    #[test]
    fn curves_fit_the_supported_range() {
        assert_eq!(super::PRESETS.len(), 18);
        assert!(super::PRESETS
            .iter()
            .all(|p| p.gains.iter().all(|v| (-12. ..=12.).contains(v))));
    }
}
