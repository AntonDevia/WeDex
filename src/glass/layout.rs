use super::{scene::Node, Page};
use eframe::egui::{pos2, vec2, Rect, Vec2};

#[derive(Clone, Copy)]
pub struct Layout {
    pub mobile: bool,
    pub size: Vec2,
    pub sidebar: f32,
    pub margin: f32,
    pub surface_x: f32,
    pub main_x: f32,
    pub main_width: f32,
    pub panel: bool,
    pub player_y: f32,
    pub player_h: f32,
    pub compact: bool,
}
impl Layout {
    pub fn for_page(size: Vec2, page: Page) -> Self {
        let mut layout = Self::new(size);
        if matches!(page, Page::Search | Page::Artist) {
            layout.panel = false;
            let padding = if size.x < 640. { 16. } else { 40. };
            layout.main_width = (size.x - layout.margin - padding - layout.main_x).max(160.);
        }
        layout
    }
    pub fn new(size: Vec2) -> Self {
        Self::for_device(size, crate::platform::mobile())
    }
    fn for_device(size: Vec2, mobile: bool) -> Self {
        let margin = if size.x < 640. { 8. } else { 24. };
        let compact = size.x < 1000.;
        let sidebar = if mobile {
            0.
        } else if compact {
            56.
        } else {
            184.
        };
        let surface_x = if mobile {
            margin
        } else {
            margin + sidebar + 16.
        };
        let padding = if size.x < 640. { 16. } else { 40. };
        let main_x = surface_x + padding;
        let panel = size.x >= 1280.;
        let main_width = (if panel {
            size.x - 332. - 58. - main_x
        } else {
            size.x - margin - padding - main_x
        })
        .max(160.);
        let player_w = size.x - margin - surface_x;
        let player_h = if mobile {
            64.
        } else if player_w < 700. {
            164.
        } else {
            112.
        };
        let player_y = size.y - margin - player_h - if mobile { 72. } else { 0. };
        Self {
            mobile,
            size,
            sidebar,
            margin,
            surface_x,
            main_x,
            main_width,
            panel,
            player_y,
            player_h,
            compact,
        }
    }
    pub fn list_rect(&self, key: &str, old: Rect) -> Rect {
        if matches!(key, "results" | "artist") {
            return old;
        }
        let y = if self.size.y < 700. {
            match key {
                "favorites" => 272.,
                "playlist" => 356.,
                "results" => 356.,
                _ => old.min.y,
            }
        } else {
            old.min.y
        };
        Rect::from_min_size(
            pos2(self.main_x, y),
            vec2(self.main_width, (self.player_y - 24. - y).max(180.)),
        )
    }
    pub fn columns(&self) -> usize {
        (((self.main_width + 24.) / if self.main_width < 600. { 168. } else { 200. }).floor()
            as usize)
            .clamp(1, 6)
    }
    fn grid_metrics(&self, count: usize) -> (usize, f32) {
        let cols = self.columns().min(count.max(1));
        (
            cols,
            (self.main_width - 24. * (cols - 1) as f32) / cols as f32,
        )
    }
    pub fn root(&self, root: &mut Node, page: Page, scroll: f32) -> f32 {
        root.width = self.size.x;
        root.height = self.size.y;
        for n in &mut root.children {
            match n.name.as_str() {
                "Ambient light" => {
                    n.width = self.size.x;
                    n.height = self.size.y;
                }
                "Sidebar" => self.menu(n, page),
                "Content surface" => {
                    n.move_to(self.surface_x, self.margin);
                    n.width = self.size.x - self.margin - self.surface_x;
                    n.height = self.player_y - self.margin - 24.;
                }
                "Floating player" => self.player(n),
                "Now playing panel" => {
                    n.visible = self.panel;
                    n.move_to(self.size.x - 332., 160.);
                    n.height = (self.player_y - 200.).max(420.);
                }
                "button/sliders-horizontal" => n.move_to(self.size.x - self.margin - 128., 48.),
                "button/list-music" => n.move_to(self.size.x - self.margin - 76., 48.),
                "Reset Sonora" => n.move_to(self.size.x - self.margin - 180., 48.),
                "button/ellipsis" => {
                    n.move_to(self.size.x - 120., 448.);
                    n.visible = self.panel;
                }
                "МУЗЫКА БЕЗ ЛИШНЕГО" | "Поиск" | "Медиатека" => {
                    n.move_to(self.main_x, 60.)
                }
                _ => {}
            }
        }
        if page == Page::Sonora {
            self.sonora(root);
            return 0.;
        }
        // Search owns one track viewport. It must never acquire a second,
        // outer page scroll on smaller windows or higher Windows scaling.
        if matches!(page, Page::Search | Page::Artist) {
            return 0.;
        }
        let scrollable = true;
        let mut content = Vec::new();
        let has_list = root.children.iter().any(|n| n.name.starts_with("list/"));
        let mut bottom: f32 = 112.;
        let has_search = root.find("Search field").is_some();
        let mut suggestion_delta = 0.;
        if let Some(grid) = root.find("Album suggestions") {
            let (cols, size) = self.grid_metrics(grid.children.len());
            let height = (grid.children.len().div_ceil(cols) as f32) * (size + 66. + 24.) - 24.;
            suggestion_delta = height - grid.height;
        }
        for mut n in std::mem::take(&mut root.children) {
            let fixed = matches!(
                n.name.as_str(),
                "Ambient light"
                    | "Sidebar"
                    | "Content surface"
                    | "Floating player"
                    | "Now playing panel"
                    | "button/sliders-horizontal"
                    | "button/list-music"
                    | "button/ellipsis"
                    | "МУЗЫКА БЕЗ ЛИШНЕГО"
                    | "Поиск"
                    | "Медиатека"
            );
            if fixed {
                root.children.push(n);
                continue;
            }
            if n.name.starts_with("list/") {
                bottom = bottom.max(n.y + n.height);
                content.push(n);
                continue;
            }
            let old_y = n.y;
            n.move_to(self.main_x + (n.x - 264.), n.y);
            if n.name.starts_with("track/") {
                let delta = self.main_width - n.width;
                n.width = self.main_width;
                for child in &mut n.children {
                    if child.name == "Save track" || child.kind == "TEXT" {
                        child.move_to(child.x + delta, child.y);
                    }
                    if child.kind == "TEXT" && self.main_width < 500. {
                        child.visible = false;
                    }
                }
                if let Some(labels) = n.find_mut("Track labels") {
                    labels.width = if self.main_width < 500. {
                        self.main_width - 120.
                    } else {
                        labels.width + delta
                    };
                    let width = labels.width;
                    labels.visit(&mut |t| {
                        if t.kind == "TEXT" {
                            t.width = width;
                        }
                    });
                }
            }
            if n.width >= 780. && n.width <= 790. {
                n.width = self.main_width;
            }
            if self.size.x < 640. && n.kind == "TEXT" && n.font_size >= 28. {
                n.font_size = 28.;
                n.natural_width = 0.;
                n.width = self.main_width;
            }
            if n.kind == "TEXT" {
                n.width = n
                    .width
                    .min((self.main_width - (n.x - self.main_x)).max(60.));
            }
            if old_y >= 632. && has_search {
                n.move_to(n.x, n.y + suggestion_delta);
            }
            match n.name.as_str() {
                "Search field" => {
                    n.width = self.main_width;
                    if let Some(mic) = n.find_mut("button/mic") {
                        mic.move_to(self.main_width - 52., 12.);
                    }
                    for t in &mut n.children {
                        if t.kind == "TEXT" {
                            t.width = (n.width - 104.).max(60.);
                        }
                    }
                }
                "Best match" => {
                    n.visible = self.size.y >= 700.;
                    n.width = self.main_width;
                    if let Some(labels) = n.find_mut("Best match title") {
                        labels.width = (self.main_width - 220.).max(80.);
                        for t in &mut labels.children {
                            if t.kind == "TEXT" {
                                t.width = labels.width;
                            }
                        }
                    }
                    if let Some(play) = n.find_mut("Play best match") {
                        play.move_to(self.main_width - 68., 60.);
                    }
                    if self.main_width < 400. {
                        if let Some(art) = n.find_mut("Best match artwork") {
                            art.width = 80.;
                            art.height = 80.;
                        }
                        if let Some(labels) = n.find_mut("Best match title") {
                            labels.move_to(120., 24.);
                            labels.width = self.main_width - 144.;
                            for t in &mut labels.children {
                                if t.kind == "TEXT" {
                                    t.width = labels.width;
                                    if t.font_size >= 24. {
                                        t.font_size = 18.;
                                        t.natural_width = 0.;
                                    }
                                }
                            }
                        }
                    }
                }
                "Album suggestions" => {
                    self.grid(&mut n);
                }
                "Playlist grid" | "All playlists" => {
                    self.grid(&mut n);
                    n.clips_content = false;
                }
                "Favorite collection" => {
                    n.width = self.main_width;
                    if let Some(art) = n.find_mut("Collection cover") {
                        art.visible = self.main_width > 560.;
                        art.move_to(self.main_width - 196., -12.);
                    }
                    for t in &mut n.children {
                        if t.kind == "TEXT" {
                            t.width = (self.main_width - 48.).max(80.);
                            if t.font_size >= 24. && self.main_width < 480. {
                                t.font_size = 19.;
                                t.natural_width = 0.;
                            }
                        }
                    }
                }
                "Library filters" => {
                    let gap = 4.;
                    let widths = [
                        self.main_width * 0.25,
                        self.main_width * 0.45,
                        self.main_width * 0.30 - 8.,
                    ];
                    let mut x = 0.;
                    for (i, tab) in n.children.iter_mut().enumerate() {
                        let width = widths[i.min(2)];
                        tab.move_to(x, 0.);
                        tab.width = width;
                        tab.visit(&mut |t| {
                            if t.kind == "TEXT" {
                                t.move_to(0., t.y);
                                t.width = width;
                                t.text_align_horizontal = "CENTER".into();
                                t.natural_width = 0.;
                                if self.main_width < 400. {
                                    t.font_size = 11.;
                                }
                            }
                        });
                        x += width + gap;
                    }
                }
                _ => {}
            }
            if page == Page::Playlist {
                self.playlist_header(&mut n);
            }
            if n.kind == "TEXT" && matches!(n.name.as_str(), "ТРЕК" | "ИСТОЧНИК") {
                let key = if page == Page::Playlist {
                    "playlist"
                } else {
                    "favorites"
                };
                let old = Rect::from_min_size(
                    pos2(264., if page == Page::Playlist { 430. } else { 334. }),
                    vec2(786., 200.),
                );
                let y = self.list_rect(key, old).min.y - 32.;
                if n.name == "ИСТОЧНИК" {
                    n.visible = self.main_width >= 500.;
                    n.move_to(self.main_x + self.main_width - 148., y);
                    n.width = 100.;
                } else {
                    n.move_to(self.main_x + 12., y);
                }
            }
            if self.size.y < 700. {
                if old_y == 112. {
                    n.move_to(n.x, 100.);
                }
                if old_y == 164. {
                    n.move_to(n.x, 148.);
                }
                if old_y == 208. {
                    n.move_to(n.x, 184.);
                }
                if old_y == 266. {
                    n.move_to(n.x, 232.);
                }
                if n.name == "Search filters" {
                    n.move_to(n.x, 252.);
                }
                if n.name == "Версии в каталогах" {
                    n.move_to(n.x, 320.);
                }
            }
            bottom = bottom.max(n.y + n.height);
            content.push(n);
        }
        let top = 96.;
        let viewport_h = (self.player_y - 24. - top).max(48.);
        let max_scroll = if scrollable {
            (bottom - top - viewport_h + if has_list { 0. } else { 24. }).max(0.)
        } else {
            0.
        };
        let mut group = frame(
            "Main content",
            self.main_x,
            top,
            self.main_width,
            viewport_h,
        );
        group.clips_content = true;
        for mut n in content {
            n.move_to(n.x - self.main_x, n.y - top - scroll.min(max_scroll));
            group.children.push(n);
        }
        root.children.push(group);
        max_scroll
    }
    fn grid(&self, n: &mut Node) {
        let (cols, size) = self.grid_metrics(n.children.len());
        let row_h = size + 66. + 24.;
        n.width = self.main_width;
        n.height = (n.children.len().div_ceil(cols) as f32 * row_h - 24.).max(0.);
        for (i, card) in n.children.iter_mut().enumerate() {
            card.move_to((i % cols) as f32 * (size + 24.), (i / cols) as f32 * row_h);
            card.width = size;
            card.height = size + 66.;
            if let Some(cover) = card.find_mut("Cover") {
                cover.width = size;
                cover.height = size;
            }
            if let Some(caption) = card.find_mut("Caption") {
                caption.move_to(0., size + 12.);
                caption.width = size;
                for text in &mut caption.children {
                    if text.kind == "TEXT" {
                        text.width = size;
                    }
                }
            }
        }
    }
    fn menu(&self, n: &mut Node, page: Page) {
        if self.mobile {
            n.move_to(self.margin, self.size.y - self.margin - 64.);
            n.width = self.size.x - self.margin * 2.;
            n.height = 64.;
            n.corner_radius = 24.;
            n.children.clear();
            let width = (n.width - 16.) / 4.;
            for (i, (label, asset, control, active)) in [
                (
                    "Sonora",
                    "audio-lines",
                    super::Control::Navigate(Page::Sonora),
                    page == Page::Sonora,
                ),
                (
                    "Поиск",
                    "search",
                    super::Control::Navigate(Page::Search),
                    matches!(page, Page::Search | Page::Artist),
                ),
                (
                    "Медиатека",
                    "library",
                    super::Control::Navigate(Page::Library),
                    !matches!(page, Page::Sonora | Page::Search | Page::Artist),
                ),
                (
                    "Настройки",
                    "settings-2",
                    super::Control::Overlay(super::Overlay::Settings),
                    false,
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let mut tab = super::panels::frame(
                    &format!("mobile-nav/{label}"),
                    8. + i as f32 * width,
                    6.,
                    width,
                    52.,
                );
                tab.command = Some(control);
                tab.corner_radius = 18.;
                if active {
                    tab.fills = vec![super::panels::fill([0.91, 1., 0.64], 0.28)];
                }
                tab.children
                    .push(super::panels::icon(asset, width / 2. - 10., 6., 20.));
                let mut text = super::panels::text(label, 0., 31., width, 10., false);
                text.text_align_horizontal = "CENTER".into();
                tab.children.push(text);
                n.children.push(tab);
            }
            return;
        }
        n.move_to(self.margin, self.margin);
        n.width = self.sidebar;
        n.height = self.size.y - 2. * self.margin;
        let height = n.height;
        if let Some(footer) = n.find_mut("Account and settings") {
            footer.height = if self.compact { 152. } else { 96. };
            footer.move_to(footer.x, height - if self.compact { 180. } else { 112. });
            if let Some(settings) = footer.find_mut("button/Настройки") {
                settings.move_to(settings.x, if self.compact { 96. } else { 48. });
                if self.compact {
                    settings.height = 40.;
                    settings.visit(&mut |node| {
                        if node.asset.is_some() {
                            node.move_to(10., 10.);
                            node.width = 20.;
                            node.height = 20.;
                        }
                    });
                }
            }
            if let Some(providers) = footer.find_mut("Connected providers") {
                providers.move_to(if self.compact { 10. } else { 0. }, 8.);
            }
        }
        if self.compact {
            for child in &mut n.children {
                child.move_to(8., child.y);
                child.width = 40.;
            }
            if let Some(brand) = n.find_mut("Brand") {
                if let Some(logo) = brand.find_mut("wedex / Wordmark") {
                    logo.asset = Some("app-symbol".into());
                    logo.width = 36.;
                    logo.height = 36.;
                    logo.move_to(2., 4.);
                }
                brand.visit(&mut |t| {
                    if t.name == "МУЗЫКА БЛИЖЕ" {
                        t.move_to(0., 48.);
                        t.width = 40.;
                        t.font_size = 9.;
                    }
                });
            }
            if let Some(nav) = n.find_mut("Navigation") {
                nav.move_to(8., 112.);
                for tab in &mut nav.children {
                    tab.width = 40.;
                    tab.visit(&mut |t| {
                        if t.kind == "TEXT" {
                            t.visible = false;
                        }
                        if t.name.starts_with("icon/") {
                            t.move_to(10., 14.);
                        }
                    });
                }
            }
            if let Some(footer) = n.find_mut("Account and settings") {
                footer.visit(&mut |t| {
                    if t.kind == "TEXT" {
                        t.visible = false;
                    }
                });
                for row in &mut footer.children {
                    row.width = 40.;
                }
                if let Some(avatar) = footer.find_mut("Avatar") {
                    avatar.move_to(6., 2.);
                }
                if let Some(providers) = footer.find_mut("Connected providers") {
                    for (index, logo) in providers.children.iter_mut().enumerate() {
                        logo.move_to((index % 2) as f32 * 20., (index / 2) as f32 * 22.);
                    }
                }
            }
        }
    }
    fn sonora(&self, root: &mut Node) {
        let landscape = self.mobile && self.size.x > self.size.y && self.size.y < 600.;
        let content_w = self.size.x - self.margin - self.surface_x;
        let center = self.surface_x + content_w / 2.;
        if let Some(title) = root.find_mut("sonora") {
            title.move_to(self.main_x, 68.);
            if content_w < 600. {
                title.font_size = 54.;
                title.natural_width = 0.;
            }
            if self.size.y < 600. {
                title.move_to(self.main_x, 68.);
                title.font_size = 44.;
                title.natural_width = 0.;
            }
        }
        if landscape {
            if let Some(title) = root.find_mut("sonora") {
                title.move_to(self.main_x, 8.);
                title.width = 210.;
            }
            for (name, x) in [
                ("Reset Sonora", 0.),
                ("button/sliders-horizontal", 52.),
                ("button/list-music", 104.),
            ] {
                if let Some(button) = root.find_mut(name) {
                    button.move_to(self.main_x + x, 132.);
                }
            }
        }
        if self.mobile && !landscape {
            if let Some(title) = root.find_mut("sonora") {
                title.move_to(self.main_x, 42.);
                title.font_size = 36.;
                title.natural_width = 0.;
                title.width = (content_w - 156.).max(100.);
            }
        }
        if let Some(sub) = root.find_mut("Ваша музыка. Новые маршруты.") {
            sub.move_to(self.main_x, if content_w < 600. { 188. } else { 213. });
            sub.width = self.main_width;
            sub.visible = false;
        }
        if let Some(label) = root.find_mut("МУЗЫКА БЕЗ ЛИШНЕГО") {
            label.visible = false;
        }
        let reason_y = self.player_y - 72.;
        if let Some(reason) = root.find_mut("Recommendation reason") {
            reason.move_to(
                if landscape {
                    self.main_x
                } else {
                    self.surface_x + 16.
                },
                if landscape { 76. } else { reason_y },
            );
            reason.width = if landscape {
                210.
            } else {
                (content_w - 32.).max(120.)
            };
        }
        if let Some(stage) = root.find_mut("Cover flow · swipe") {
            stage.move_to(
                self.surface_x + 16.,
                if content_w < 600. { 168. } else { 180. },
            );
            stage.width = content_w - 32.;
            if self.size.y < 600. {
                stage.move_to(stage.x, 156.);
            }
            if landscape {
                stage.move_to(self.main_x + 232., 16.);
                stage.width = self.size.x - self.margin - stage.x;
            }
            stage.clips_content = !self.mobile || landscape;
            let starting = stage
                .children
                .iter()
                .any(|n| n.name == "Album/sonora-start");
            stage.height = (if starting || landscape {
                self.player_y - 24.
            } else {
                reason_y - 16.
            } - stage.y)
                .max(72.);
            let main_scale = (stage.width * if content_w < 600. { 0.60 } else { 0.30 } / 312.)
                .min(stage.height * 0.88 / 390.)
                .min(1.4);
            let cx = if landscape {
                stage.width / 2.
            } else {
                center - stage.x
            };
            for (card, offset) in stage.children.iter_mut().zip([-2_i32, 2, -1, 1, 0]) {
                let offset = if card.name == "Album/sonora-start" {
                    0
                } else {
                    offset
                };
                let normalize = 312. / card.width;
                card.scale_geometry(normalize);
                let ratio = match offset.abs() {
                    0 => 1.,
                    1 => 0.84,
                    _ => 0.66,
                };
                let scale = main_scale * ratio;
                let angle = (-(offset.signum()) as f32
                    * if offset.abs() == 1 { 5.0_f32 } else { 8.0 })
                .to_radians();
                let a = angle.cos() * scale;
                let b = angle.sin() * scale;
                let c = -b;
                let d = a;
                let x = cx + offset as f32 * 312. * main_scale * 0.62 - (a * 312. + c * 390.) / 2.;
                let y = stage.height / 2. + offset.abs() as f32 * 10. * main_scale
                    - (b * 312. + d * 390.) / 2.;
                card.x = x;
                card.y = y;
                card.relative_transform = Some([[a, c, x], [b, d, y]]);
            }
        }
    }
    fn playlist_header(&self, n: &mut Node) {
        if self.main_width >= 600. {
            return;
        }
        match n.name.as_str() {
            "Playlist artwork" => {
                n.width = 104.;
                n.height = 104.;
            }
            "ВАШ ПЛЕЙЛИСТ" | "Ночной маршрут" => {
                n.move_to(self.main_x + 124., n.y);
                n.width = (self.main_width - 124.).max(80.);
                if n.font_size >= 28. {
                    n.font_size = 24.;
                    n.natural_width = 0.;
                }
            }
            "6 треков · из Яндекс Музыки" => {
                n.move_to(self.main_x + 124., 272.);
                n.width = self.main_width - 124.;
            }
            "button/Слушать" => {
                n.move_to(self.main_x, 310.);
                if self.main_width < 400. {
                    n.width = (self.main_width - 8.) / 2.;
                    center_labels(n);
                }
            }
            "button/Перемешать" => {
                n.move_to(
                    self.main_x
                        + if self.main_width < 400. {
                            (self.main_width + 8.) / 2.
                        } else {
                            160.
                        },
                    310.,
                );
                if self.main_width < 400. {
                    n.width = (self.main_width - 8.) / 2.;
                    center_labels(n);
                    n.visit(&mut |t| {
                        if t.kind == "TEXT" {
                            t.font_size = 12.;
                        }
                    });
                }
            }
            "button/ellipsis" => n.visible = false,
            _ => {}
        }
    }
    fn player(&self, n: &mut Node) {
        n.move_to(self.surface_x, self.player_y);
        n.width = self.size.x - self.margin - self.surface_x;
        n.height = self.player_h;
        let w = n.width;
        n.corner_radius = 24.;
        if self.mobile {
            n.command = Some(super::Control::Zen(true));
            n.children.retain(|child| {
                matches!(
                    child.name.as_str(),
                    "Current artwork"
                        | "Now playing labels"
                        | "Save current track"
                        | "Play / pause"
                )
            });
            place(n, "Current artwork", 12., 12., 40., 40.);
            place(n, "Now playing labels", 64., 12., (w - 184.).max(64.), 40.);
            place(n, "Save current track", w - 108., 10., 44., 44.);
            place(n, "Play / pause", w - 56., 10., 44., 44.);
            if let Some(labels) = n.find_mut("Now playing labels") {
                labels.command = Some(super::Control::Zen(true));
                let width = labels.width;
                for text in &mut labels.children {
                    if text.kind == "TEXT" {
                        text.width = width;
                    }
                }
            }
            if let Some(play) = n.find_mut("Play / pause") {
                play.fills.clear();
                play.strokes.clear();
            }
            if let Some(like) = n.find_mut("Save current track") {
                if like.command.is_none() {
                    like.command = Some(super::Control::Noop);
                }
            }
            return;
        }
        let compact = w < 700.;
        let landscape = self.mobile && self.size.x > self.size.y && self.size.y < 600.;
        let row_y = if landscape {
            4.
        } else if compact {
            50.
        } else {
            12.
        };
        place(
            n,
            "Current artwork",
            16.,
            if landscape { 8. } else { 12. },
            40.,
            40.,
        );
        place(
            n,
            "Now playing labels",
            68.,
            if landscape { 8. } else { 12. },
            if compact {
                w - 84.
            } else {
                (w * 0.18).clamp(64., 180.)
            },
            40.,
        );
        let controls = if compact {
            [
                ("Save current track", 40.),
                ("button/skip-back", 28.),
                ("Play / pause", 40.),
                ("button/skip-forward", 28.),
                ("Shuffle", 24.),
                ("Repeat", 24.),
            ]
        } else {
            [
                ("Save current track", 44.),
                ("button/skip-back", 28.),
                ("Play / pause", 44.),
                ("button/skip-forward", 28.),
                ("Shuffle", 28.),
                ("Repeat", 28.),
            ]
        };
        let gap = if compact { 8. } else { 12. };
        let total = controls.iter().map(|(_, w)| w).sum::<f32>() + gap * 5.;
        let mut x = (w - total) / 2. + gap;
        for (name, width) in controls {
            place(n, name, x, row_y + (44. - width) / 2., width, width);
            x += width + gap;
        }
        let utilities = [
            "Lyrics toggle",
            "button/list-music",
            "button/sliders-horizontal",
            "Mute",
        ];
        let (mut x, utility_y, side, gap) = if self.mobile && compact {
            ((w - 176.) / 2., 96., 32., 16.)
        } else if compact {
            ((w - 132.) / 2., 100., 24., 12.)
        } else {
            (w - 160., if landscape { 8. } else { 20. }, 28., 8.)
        };
        for name in utilities {
            place(n, name, x, utility_y, side, side);
            x += side + gap;
        }
        place(n, "Timeline", 16., self.player_h - 34., w - 32., 24.);
        if let Some(volume) = n.find_mut("Volume") {
            volume.visible = false;
        }
        if let Some(labels) = n.find_mut("Now playing labels") {
            let width = labels.width;
            for t in &mut labels.children {
                if t.kind == "TEXT" {
                    t.width = width;
                }
            }
        }
        if let Some(timeline) = n.find_mut("Timeline") {
            let width = timeline.width;
            if let Some(seek) = timeline.find_mut("Seek") {
                resize_slider(seek, (width - 84.).max(40.));
                seek.move_to(42., 0.);
            }
            if let Some(times) = timeline.find_mut("Time labels") {
                times.width = width;
                times.move_to(0., 0.);
                for text in &mut times.children {
                    text.move_to(text.x, 2.);
                }
                if let Some(total) = times.children.last_mut() {
                    total.move_to(width - total.width, 2.);
                }
            }
        }
    }

    pub fn overlay(&self, root: &mut Node, scroll: f32) -> f32 {
        root.width = self.size.x;
        root.height = self.size.y;
        if let Some(scrim) = root.find_mut("Click outside to close") {
            scrim.width = self.size.x;
            scrim.height = self.size.y;
        }
        let Some(panel) = root.find_mut("Glass dialog") else {
            return 0.;
        };
        let original_w = panel.width;
        let original_h = panel.height;
        panel.width = original_w.min(self.size.x - 2. * self.margin);
        panel.height = original_h.min(self.size.y - 2. * self.margin);
        let queue = original_w < 400.;
        panel.move_to(
            if queue {
                self.size.x - self.margin - panel.width
            } else {
                (self.size.x - panel.width) / 2.
            },
            if queue {
                self.margin
            } else {
                (self.size.y - panel.height) / 2.
            },
        );
        let width = panel.width;
        let height = panel.height;
        if let Some(close) = panel.find_mut("button/x") {
            close.move_to(width - 56., 16.);
        }
        for child in &mut panel.children {
            if child.name.starts_with("button/") && child.y > 350. {
                child.move_to(child.x, height - 68.);
            }
            if child.width > width - 56. {
                child.width = (width - 56.).max(100.);
            }
            if child.name.starts_with("Credential input/") {
                child.width = width - 56.;
            }
            if child.kind == "TEXT" {
                child.width = (width - child.x - 28.).max(80.);
            }
            if child.name.starts_with("theme/") {
                center_labels(child);
            }
        }
        if let Some(bands) = panel.find_mut("Frequency bands") {
            bands.width = width - 64.;
            let lane_w = bands.width / 10.;
            for (i, lane) in bands.children.iter_mut().enumerate() {
                lane.move_to(i as f32 * lane_w, 0.);
                lane.width = lane_w;
                lane.visit(&mut |node| {
                    if node.name.starts_with("Gain/") {
                        node.width = lane_w;
                    }
                    if node.kind == "TEXT" {
                        node.move_to(0., node.y);
                        node.width = lane_w;
                        node.natural_width = 0.;
                        node.text_align_horizontal = "CENTER".into();
                    }
                    if node.name == "Rail" {
                        node.move_to(lane_w / 2. - 1.5, node.y);
                    }
                    if node.name == "Thumb" {
                        node.move_to(lane_w / 2. - 6., node.y);
                    }
                });
            }
            let button_w = (width - 64. - 3. * 8.) / 4.;
            for (i, name) in [
                "button/Ровно",
                "button/Теплее",
                "button/Сбросить",
                "button/Готово",
            ]
            .iter()
            .enumerate()
            {
                if let Some(button) = panel.find_mut(name) {
                    button.move_to(32. + i as f32 * (button_w + 8.), height - 68.);
                    button.width = button_w;
                    center_labels(button);
                }
            }
        }
        if original_w == 560. {
            for (i, choice) in panel
                .children
                .iter_mut()
                .filter(|n| n.name.starts_with("Choice/"))
                .enumerate()
            {
                choice.width = (width - 72.) / 2.;
                choice.move_to(28. + i as f32 * (choice.width + 16.), 112.);
                choice.visit(&mut |n| {
                    if n.kind == "TEXT" {
                        n.width = (width - 104.) / 2.;
                        n.natural_width = 0.;
                        if width < 500. {
                            n.font_size = 12.;
                        }
                    }
                });
            }
        }
        for row in panel
            .children
            .iter_mut()
            .filter(|n| n.name.starts_with("Source/"))
        {
            let soundcloud = row.name.contains("SoundCloud");
            row.width = width - 56.;
            let row_width = row.width;
            if let Some(check) = row.find_mut("icon/check") {
                check.move_to(row_width - 36., 26.);
            }
            if let Some(labels) = row.find_mut("Source labels") {
                let lw = (width - 172.).max(90.);
                labels.width = lw;
                for t in &mut labels.children {
                    t.width = lw;
                }
            }
            if let Some(button) = row
                .children
                .iter_mut()
                .find(|n| n.name.starts_with("button/"))
            {
                button.width = if width < 520. { 96. } else { 180. };
                button.move_to(row.width - button.width - 12., 13.);
                center_labels(button);
            }
            if width < 520. {
                if let Some(labels) = row.find_mut("Source labels") {
                    let lw = (width - 260.).max(68.);
                    if soundcloud {
                        labels.width = lw;
                        for t in &mut labels.children {
                            t.width = lw;
                        }
                    }
                }
            }
        }
        if let Some(list) = panel.find_mut("Version list") {
            list.width = width - 56.;
            let row_width = list.width;
            for row in &mut list.children {
                row.width = row_width;
                if let Some(labels) = row.find_mut("Version labels") {
                    labels.width = (row_width - 132.).max(80.);
                    let lw = labels.width;
                    labels.visit(&mut |n| {
                        if n.kind == "TEXT" {
                            n.width = lw;
                            n.natural_width = 0.;
                        }
                    });
                }
                if let Some(preview) = row.find_mut("Preview version") {
                    preview.move_to(row_width - 96., 21.);
                }
                if let Some(select) = row.children.last_mut() {
                    select.move_to(row_width - 44., 25.);
                }
            }
        }
        for button in panel
            .children
            .iter_mut()
            .filter(|n| n.name.starts_with("button/") && n.width > 200.)
        {
            button.width = width - 56.;
            center_labels(button);
        }
        let requires_scroll = original_h > height + 1. && original_w != 720.;
        if requires_scroll {
            let top = 104.;
            let bottom = height - 88.;
            let mut body = frame("Dialog content", 0., top, width, (bottom - top).max(80.));
            body.clips_content = true;
            let mut end = top;
            let mut fixed = vec![];
            for mut n in std::mem::take(&mut panel.children) {
                if n.y >= top && !(n.name.starts_with("button/") && n.y >= height - 68.) {
                    end = end.max(n.y + n.height);
                    n.move_to(n.x, n.y - top - scroll);
                    body.children.push(n);
                } else {
                    fixed.push(n);
                }
            }
            let max = (end - top - body.height).max(0.);
            panel.children = fixed;
            panel.children.push(body);
            return max;
        }
        0.
    }
}
fn frame(name: &str, x: f32, y: f32, w: f32, h: f32) -> Node {
    let mut n = Node {
        name: name.into(),
        id: name.into(),
        kind: "FRAME".into(),
        visible: true,
        opacity: 1.,
        width: w,
        height: h,
        ..Default::default()
    };
    n.move_to(x, y);
    n
}
fn place(parent: &mut Node, name: &str, x: f32, y: f32, w: f32, h: f32) {
    if let Some(n) = parent.find_mut(name) {
        let old = n.width;
        n.move_to(x, y);
        if n.name == "Volume" {
            resize_slider(n, w);
        }
        n.width = w;
        n.height = h;
        if n.asset.is_none()
            && n.name != "Timeline"
            && n.name != "Volume"
            && n.name != "Now playing labels"
            && old > 0.
        {
            let factor = w / old;
            for child in &mut n.children {
                child.scale_geometry(factor);
            }
        }
    }
}
fn center_labels(n: &mut Node) {
    let width = n.width;
    n.visit(&mut |t| {
        if t.kind == "TEXT" {
            t.move_to(0., t.y);
            t.width = width;
            t.natural_width = 0.;
            t.text_align_horizontal = "CENTER".into();
        }
    });
}
fn resize_slider(n: &mut Node, width: f32) {
    let old = n.width.max(1.);
    let fraction = n.find("Progress").map(|p| p.width / old).unwrap_or(0.);
    n.width = width;
    if let Some(rail) = n.find_mut("Rail") {
        rail.width = width;
    }
    if let Some(progress) = n.find_mut("Progress") {
        progress.width = fraction * width;
    }
    if let Some(thumb) = n.find_mut("Thumb") {
        thumb.move_to(fraction * width - thumb.width / 2., thumb.y);
    }
}

#[cfg(test)]
mod mobile_tests {
    use super::*;
    #[test]
    fn portrait_sonora_title_shares_the_toolbar_row_and_cards_can_reach_screen_edges() {
        for size in [vec2(360., 640.), vec2(411., 838.), vec2(800., 1280.)] {
            let layout = Layout::for_device(size, true);
            let mut root = Node::default();
            root.children.push(super::super::panels::text(
                "sonora", 0., 0., 200., 54., true,
            ));
            root.children.last_mut().unwrap().name = "sonora".into();
            root.children.push(super::super::panels::frame(
                "Cover flow · swipe",
                0.,
                0.,
                100.,
                100.,
            ));
            layout.sonora(&mut root);
            let title = root.find("sonora").unwrap();
            assert!(title.y >= 36. && title.y <= 48.);
            assert!(!root.find("Cover flow · swipe").unwrap().clips_content);
        }
    }
    #[test]
    fn landscape_sonora_stage_stays_above_the_player() {
        for size in [vec2(838., 411.), vec2(780., 360.)] {
            let layout = Layout::for_device(size, true);
            let mut root = Node::default();
            let mut stage = super::super::panels::frame("Cover flow · swipe", 0., 0., 100., 100.);
            stage.children.push(super::super::panels::frame(
                "Album/sonora-start",
                0.,
                0.,
                312.,
                390.,
            ));
            root.children.push(stage);
            layout.sonora(&mut root);
            let stage = root.find("Cover flow · swipe").unwrap();
            assert!(stage.clips_content);
            assert!(stage.height >= 72.);
            assert!(stage.y + stage.height <= layout.player_y);
            assert!(stage.x + stage.width <= size.x);
        }
    }
    #[test]
    fn bottom_navigation_and_player_fit_phone_tablet_and_landscape() {
        for size in [
            vec2(360., 640.),
            vec2(411., 838.),
            vec2(800., 1280.),
            vec2(838., 411.),
        ] {
            let layout = Layout::for_device(size, true);
            assert_eq!(layout.sidebar, 0.);
            assert_eq!(layout.surface_x, layout.margin);
            let mut menu = Node::default();
            layout.menu(&mut menu, Page::Search);
            assert_eq!(menu.children.len(), 4);
            assert!(layout.player_y >= 0.);
            assert!(layout.player_y + layout.player_h <= menu.y);
            assert!(menu.y + menu.height <= size.y);
            for tab in &menu.children {
                assert!(tab.width >= 44. && tab.height >= 44.);
                assert!(tab.x >= 0. && tab.x + tab.width <= menu.width);
            }
        }
    }
}
