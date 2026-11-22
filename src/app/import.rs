use super::{App, Event};
use crate::track_import::{self, Input, Progress, Report, Source, View};
use std::sync::{
    atomic::{AtomicU64, Ordering::Relaxed},
    Arc,
};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(super) struct State {
    pub(super) view: View,
    picker: Option<(u64, Source)>,
    request: u64,
}

impl State {
    pub(super) fn cancel(&mut self) {
        if let View::Working(progress) = &self.view {
            progress.cancelled.store(true, Relaxed);
        }
        self.view = View::Idle;
        self.picker = None;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl App {
    pub(super) fn begin_import(&mut self, input: Input) {
        if self.track_import.view.busy() {
            return;
        }
        if self.store.is_none() {
            self.track_import.view = View::Error("Медиатека недоступна".into());
            return;
        }
        let progress = Arc::new(Progress::default());
        self.track_import.view = View::Working(Arc::clone(&progress));
        let request = REQUEST_ID.fetch_add(1, Relaxed);
        self.track_import.request = request;
        let favorites = self.favorites.clone();
        let offline = self.settings.offline_only;
        self.job(move |services| {
            Event::Import(
                request,
                track_import::run(&services, input, offline, favorites, &progress)
                    .map_err(|error| error.to_string()),
            )
        });
    }

    pub(super) fn pick_import_file(&mut self, source: Source) {
        if self.track_import.view.busy() {
            return;
        }
        let id = REQUEST_ID.fetch_add(1, Relaxed);
        self.track_import.picker = Some((id, source));
        self.track_import.view = View::Picking;
        #[cfg(windows)]
        {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let result = track_import::file::pick().map_err(|error| error.to_string());
                let _ = tx.send(Event::ImportFile(id, result));
            });
        }
        #[cfg(target_os = "android")]
        if crate::android::pick_import_file(id).is_err() {
            self.import_file(id, Err("Не удалось открыть список файлов".into()));
        }
        #[cfg(not(any(windows, target_os = "android")))]
        self.import_file(id, Err("Вставьте список треков из буфера обмена".into()));
    }

    pub(super) fn import_file(&mut self, id: u64, result: Result<Option<String>, String>) {
        let Some((_, source)) = self
            .track_import
            .picker
            .filter(|(request, _)| *request == id)
        else {
            return;
        };
        self.track_import.cancel();
        match result {
            Ok(Some(contents)) => self.begin_import(Input::Text(source, contents)),
            Ok(None) => {}
            Err(error) => self.track_import.view = View::Error(error),
        }
    }

    pub(super) fn finish_import(
        &mut self,
        id: u64,
        result: Result<(Vec<crate::model::Track>, Report), String>,
    ) {
        if id != self.track_import.request || !matches!(self.track_import.view, View::Working(_)) {
            return;
        }
        self.track_import.view = match result {
            Ok((tracks, mut report)) => match self
                .store
                .as_ref()
                .map(|store| store.import_favorites(&tracks))
            {
                Some(Ok((added, hidden))) => {
                    if let Some(favorites) =
                        self.store.as_ref().and_then(|store| store.favorites().ok())
                    {
                        self.favorites = favorites;
                    }
                    report.added = added;
                    report.hidden = hidden;
                    report.existing += tracks.len() - added - hidden;
                    View::Finished(Arc::new(report))
                }
                _ => View::Error("Не удалось сохранить треки в медиатеку".into()),
            },
            Err(error) => View::Error(error),
        };
    }
}
