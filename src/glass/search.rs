//! Search and artist views share a fixed header and one clipped track viewport.
use super::{panels::*, *};
use serde_json::json;

const SOURCES: [(Option<Provider>, &str, &str); 5] = [
    (None, "Все", "search"),
    (Some(Provider::Yandex), "Яндекс", "provider-yandex"),
    (Some(Provider::Hitmotop), "Hitmo", "provider-hitmo"),
    (
        Some(Provider::SoundCloud),
        "SoundCloud",
        "provider-soundcloud",
    ),
    (Some(Provider::YouTube), "YouTube", "provider-youtube"),
];

fn portrait(source: Option<&str>, x: f32, y: f32, size: f32, round: bool) -> Node {
    let mut n = frame("Artist portrait", x, y, size, size);
    n.corner_radius = if round { size / 2. } else { 14. };
    n.fills = vec![fill([0.91, 0.9, 0.89], 0.65)];
    if let Some(source) = source {
        n.fills
            .push(json!({"type":"IMAGE", "source":source, "scaleMode":"FILL"}));
    } else {
        let mut initial = text("♪", 0., size * 0.2, size, size * 0.4, false);
        initial.text_align_horizontal = "CENTER".into();
        n.children.push(initial);
    }
    n
}

fn card(name: &str, x: f32, y: f32, w: f32, h: f32) -> Node {
    let mut n = frame(name, x, y, w, h);
    n.corner_radius = 20.;
    n.clips_content = true;
    n.fills = vec![fill([1., 1., 1.], 0.52)];
    n.strokes = vec![fill([0.18, 0.16, 0.21], 0.08)];
    n.stroke_weight = 1.;
    n
}

fn artist_card(artist: &model::Artist, x: f32, y: f32, w: f32, h: f32) -> Node {
    let mut n = card("Artist result", x, y, w, h);
    n.id = format!("artist-result/{}", artist.id);
    n.command = Some(Control::Artist(artist.clone()));
    let art = (h - if h < 65. { 16. } else { 24. }).max(24.);
    n.children
        .push(portrait(artist.cover.as_deref(), 12., 12., art, true));
    let left = art + 24.;
    let font = if h < 65. { 14. } else { 19. };
    if h >= 65. {
        let mut label = text("Исполнитель", left, 14., w - left - 16., 11., false);
        label.opacity = 0.55;
        n.children.push(label);
    }
    n.children.push(text(
        &artist.name,
        left,
        if h < 65. { 14. } else { 36. },
        w - left - 14.,
        font,
        true,
    ));
    n
}

fn best_match(track: &Track, x: f32, y: f32, w: f32, h: f32) -> Node {
    let mut n = card("Best match compact", x, y, w, h);
    let art = h - 24.;
    n.children.push(portrait(
        Some(&GlassUi::source(track)),
        if h < 65. { 8. } else { 12. },
        12.,
        art,
        false,
    ));
    let left = art + 24.;
    let width = (w - left - 58.).max(32.);
    let mut label = text("Лучшее совпадение", left, 10., width, 10., false);
    label.opacity = 0.55;
    n.children.push(label);
    n.children
        .push(text(&track.title, left, 28., width, 17., true));
    if h >= 85. {
        let mut artist = text(&track.artist, left, 54., width, 12., false);
        artist.opacity = 0.6;
        n.children.push(artist);
    }
    n.children.push(icon_button(
        "play",
        w - 50.,
        (h - 36.) / 2.,
        36.,
        Control::Play(track.clone()),
        true,
    ));
    n
}

pub(super) fn compose(
    ui: &mut GlassUi,
    ctx: &egui::Context,
    m: &Model,
    root: &mut Node,
    results: bool,
) {
    root.children.retain(|n| {
        matches!(
            n.name.as_str(),
            "Ambient light" | "Sidebar" | "Content surface" | "Floating player"
        )
    });
    if ui.page == Page::Artist {
        artist_view(ui, ctx, m, root);
        return;
    }
    let l = ui.layout;
    let short = l.size.y < 700.;
    let tiny = l.size.y < 600.;
    let wide = l.main_width >= 620.;
    let y = if tiny {
        8.
    } else if short {
        24.
    } else {
        38.
    };
    let title_size = if short {
        44.
    } else if wide {
        76.
    } else {
        54.
    };
    let mut title = text(
        "Поиск",
        l.main_x,
        y,
        l.main_width.min(244.),
        title_size,
        false,
    );
    title.name = "Search title".into();
    title.font_name = json!({"family":"Medeo Sans Display", "style":"Medium"});
    root.children.push(title);

    let field_y = if wide {
        y + 16.
    } else {
        y + title_size * 1.16 + if tiny { 4. } else { 8. }
    };
    let field_x = if wide { l.main_x + 250. } else { l.main_x };
    let field_w = if wide {
        l.main_width - 250.
    } else {
        l.main_width
    };
    let field_h = if tiny { 48. } else { 56. };
    let mut field = card("Search field", field_x, field_y, field_w, field_h);
    field.command = Some(Control::Noop);
    field.corner_radius = 28.;
    let mut search = icon_button("search", 8., 10., 36., Control::Search, false);
    search.fills.clear();
    field.children.push(search);
    let focused = ctx.memory(|memory| memory.has_focus(egui::Id::new("music-query")));
    if !focused {
        let mut value = text(
            if ui.query.is_empty() {
                "Трек или исполнитель"
            } else {
                &ui.query
            },
            44.,
            17.,
            field_w - 104.,
            14.,
            false,
        );
        value.opacity = if ui.query.is_empty() { 0.48 } else { 1. };
        field.children.push(value);
    }
    let mut mic = icon_button("mic", field_w - 48., 8., 40., Control::Mic, false);
    mic.fills.clear();
    field.children.push(mic);
    root.children.push(field);
    let mut next_y = (field_y + field_h).max(y + title_size * 1.16) + if tiny { 3. } else { 16. };

    if results {
        let inline_artist = tiny && wide && !m.artists.is_empty();
        let selected = SOURCES.get(ui.search_filter).and_then(|source| source.0);
        let tracks = m
            .results
            .iter()
            .filter(|t| selected.is_none_or(|p| t.provider == p))
            .cloned()
            .collect::<Vec<_>>();
        let h = if tiny {
            40.
        } else if short {
            76.
        } else {
            96.
        };
        let artists = m
            .artists
            .iter()
            .take(if wide { 2 } else { 1 })
            .collect::<Vec<_>>();
        if !artists.is_empty() {
            let best_w = if wide && !tiny && !tracks.is_empty() {
                l.main_width * 0.38
            } else {
                0.
            };
            if best_w > 0. {
                root.children
                    .push(best_match(&tracks[0], l.main_x, next_y, best_w, h));
            }
            let left = best_w + if best_w > 0. { 12. } else { 0. };
            let count = if inline_artist || l.main_width - left < 540. {
                1
            } else {
                artists.len()
            };
            let width = if inline_artist {
                210.
            } else {
                (l.main_width - left - (count - 1) as f32 * 12.) / count as f32
            };
            for (i, artist) in artists.into_iter().take(count).enumerate() {
                let mut card = artist_card(
                    artist,
                    l.main_x + left + i as f32 * (width + 12.),
                    next_y,
                    width,
                    h,
                );
                if i + 1 == count && m.artists.len() > count {
                    for child in &mut card.children {
                        if child.kind == "TEXT" {
                            child.width = (child.width - 38.).max(24.);
                        }
                    }
                    let mut more = icon_button(
                        "chevron-right",
                        width - 42.,
                        (h - 32.) / 2.,
                        32.,
                        Control::Overlay(Overlay::Artists),
                        false,
                    );
                    more.name = "All artists".into();
                    card.children.push(more);
                }
                root.children.push(card);
            }
            if !inline_artist {
                next_y += h + if tiny { 8. } else { 12. };
            }
        } else if let Some(track) = tracks.first().filter(|_| !tiny) {
            root.children.push(best_match(
                track,
                l.main_x,
                next_y,
                l.main_width.min(560.),
                h,
            ));
            next_y += h + 12.;
        }
        let inset = if inline_artist { 222. } else { 0. };
        let width = l.main_width - inset;
        let filter_w = (width - 4. * 6.) / 5.;
        let mut filters = frame(
            "Search filters",
            l.main_x + inset,
            next_y + if inline_artist { 7. } else { 0. },
            width,
            34.,
        );
        for (i, (provider, label, asset)) in SOURCES.iter().enumerate() {
            let count = m
                .results
                .iter()
                .filter(|t| provider.is_none_or(|p| t.provider == p))
                .count();
            let label = if filter_w >= 112. {
                format!("{label} · {count}")
            } else {
                label.to_string()
            };
            let mut tab = if filter_w < 72. {
                let mut tab = icon_button(
                    asset,
                    0.,
                    0.,
                    32.,
                    Control::Source(format!("filter/{i}")),
                    ui.search_filter == i,
                );
                tab.width = filter_w;
                tab.children[0].move_to((filter_w - 18.) / 2., 7.);
                tab.children[0].width = 18.;
                tab.children[0].height = 18.;
                tab
            } else {
                button(
                    &label,
                    0.,
                    0.,
                    filter_w,
                    34.,
                    Control::Source(format!("filter/{i}")),
                    ui.search_filter == i,
                )
            };
            tab.name = format!("Search source/{i}");
            tab.move_to(i as f32 * (filter_w + 6.), 0.);
            filters.children.push(tab);
        }
        root.children.push(filters);
        next_y += if inline_artist {
            52.
        } else if tiny {
            36.
        } else {
            44.
        };
        track_list(
            ui,
            ctx,
            m,
            root,
            &tracks,
            (next_y, "results"),
            if m.searching {
                "Ищем музыку…"
            } else {
                "В этом источнике ничего не найдено"
            },
        );
    } else {
        let mut heading = text(
            "Недавно искали",
            l.main_x,
            next_y + 8.,
            l.main_width,
            18.,
            true,
        );
        heading.name = "Search recent heading".into();
        root.children.push(heading);
        next_y += 48.;
        let tracks = if m.preview {
            m.favorites.clone()
        } else {
            m.settings.recent_searches.clone()
        };
        track_list(
            ui,
            ctx,
            m,
            root,
            &tracks,
            (next_y, "results"),
            "Найдите любимый трек или исполнителя",
        );
    }
}

pub(super) fn artists_panel(ui: &mut GlassUi, m: &Model) -> Node {
    let (mut root, mut panel) = panel(ui, "Исполнители", 600., 640.);
    let mut body = body(ui, &panel, m.artists.len() as f32 * 88.);
    let width = body.width - 8.;
    if let Some(content) = body.children.first_mut() {
        for (i, artist) in m.artists.iter().enumerate() {
            content
                .children
                .push(artist_card(artist, 0., i as f32 * 88., width, 80.));
        }
    }
    panel.children.push(body);
    root.children.push(panel);
    root
}

fn track_list(
    ui: &mut GlassUi,
    ctx: &egui::Context,
    m: &Model,
    root: &mut Node,
    tracks: &[Track],
    location: (f32, &'static str),
    empty: &str,
) {
    let (y, key) = location;
    let l = ui.layout;
    let rect = Rect::from_min_max(
        pos2(l.main_x, y),
        pos2(l.main_x + l.main_width, (l.player_y - 24.).max(y + 32.)),
    );
    ui.list(ctx, root, m, tracks, ListViewport { rect, key }, false);
    if tracks.is_empty() {
        let list = root.children.last_mut().unwrap();
        list.children.clear();
        let mut label = text(empty, 12., 12., l.main_width - 24., 14., false);
        label.opacity = 0.55;
        list.children.push(label);
    }
}

fn artist_view(ui: &mut GlassUi, ctx: &egui::Context, m: &Model, root: &mut Node) {
    let l = ui.layout;
    let mut back = button(
        "К результатам",
        l.main_x,
        36.,
        142.,
        34.,
        Control::Navigate(Page::Search),
        false,
    );
    back.name = "Back to search".into();
    root.children.push(back);
    let Some(catalog) = &m.artist else {
        return;
    };
    let artist = &catalog.artist;
    let short = l.size.y < 700.;
    let narrow = l.main_width < 450.;
    let tiny = l.size.y < 600.;
    let photo = if tiny {
        48.
    } else if short || narrow {
        76.
    } else {
        156.
    };
    let top = if tiny { 74. } else { 86. };
    let left = l.main_x + photo + if narrow { 16. } else { 28. };
    let width = l.main_width - (left - l.main_x);
    let mut hero = portrait(artist.cover.as_deref(), l.main_x, top, photo, true);
    hero.name = "Artist hero portrait".into();
    root.children.push(hero);
    let mut label = text("Исполнитель", left, top + 2., width, 12., true);
    label.opacity = 0.55;
    root.children.push(label);
    let mut name = text(
        &artist.name,
        left,
        top + if tiny { 20. } else { 22. },
        width,
        if narrow || tiny {
            24.
        } else if short {
            32.
        } else {
            54.
        },
        true,
    );
    name.name = "Artist name".into();
    name.font_name = json!({"family":"Medeo Sans Display", "style":"Medium"});
    root.children.push(name);
    let button_h = if tiny { 32. } else { 40. };
    let buttons_y = if short || narrow {
        top + photo + if tiny { 8. } else { 12. }
    } else {
        top + 120.
    };
    if !short && !narrow {
        let stats = artist
            .listeners
            .map(|n| format!("{} слушателей за месяц", number(n)))
            .unwrap_or_else(|| "Яндекс Музыка".into());
        let mut label = text(&stats, left, top + 91., width, 13., false);
        label.opacity = 0.6;
        root.children.push(label);
    }
    let buttons_x = if short || narrow { l.main_x } else { left };
    let mut play = button(
        "Слушать",
        buttons_x,
        buttons_y,
        if narrow { 100. } else { 130. },
        button_h,
        if catalog.tracks.is_empty() {
            Control::Noop
        } else {
            Control::Listen(false)
        },
        true,
    );
    play.name = "Listen artist".into();
    if catalog.tracks.is_empty() {
        play.opacity = 0.5;
    }
    root.children.push(play);
    let other_x = buttons_x + if narrow { 108. } else { 142. };
    let other_w = (l.main_x + l.main_width - other_x).min(190.);
    if other_w >= 96. {
        root.children.push(button(
            if other_w < 160. {
                "Другие источники"
            } else {
                "В других источниках"
            },
            other_x,
            buttons_y,
            other_w,
            button_h,
            Control::ArtistSources(artist.name.clone()),
            false,
        ));
    }
    let bottom = (top + photo).max(buttons_y + button_h)
        + if tiny {
            8.
        } else if short {
            12.
        } else {
            24.
        };
    let count = catalog
        .total
        .or(artist.tracks)
        .map(|n| {
            if l.main_width < 300. {
                format!("Треки · {n}")
            } else {
                format!("Все треки · {n}")
            }
        })
        .unwrap_or_else(|| "Все треки".into());
    root.children.push(text(
        &count,
        l.main_x,
        bottom,
        (l.main_width
            - if catalog.loading || catalog.next_page.is_some() {
                142.
            } else {
                0.
            })
        .max(80.),
        16.,
        true,
    ));
    if catalog.loading {
        let mut label = text(
            "Загружаем…",
            l.main_x + l.main_width - 130.,
            bottom + 2.,
            130.,
            12.,
            false,
        );
        label.opacity = 0.55;
        root.children.push(label);
    } else if catalog.next_page.is_some() {
        root.children.push(button(
            if catalog.failed {
                "Повторить"
            } else {
                "Загрузить ещё"
            },
            l.main_x + l.main_width - 134.,
            bottom - 2.,
            134.,
            28.,
            Control::ArtistMore,
            false,
        ));
    }
    track_list(
        ui,
        ctx,
        m,
        root,
        &catalog.tracks,
        (bottom + if tiny { 28. } else { 36. }, "artist"),
        if catalog.loading {
            "Загружаем треки исполнителя…"
        } else if catalog.failed {
            "Не удалось загрузить треки. Попробуйте ещё раз"
        } else {
            "Доступных треков пока нет"
        },
    );
}

fn number(n: u64) -> String {
    n.to_string()
        .chars()
        .rev()
        .enumerate()
        .flat_map(|(i, c)| {
            if i > 0 && i % 3 == 0 {
                vec![' ', c]
            } else {
                vec![c]
            }
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}
