//! Platform file boundary. Editor and loaders only see named byte buffers.
#[cfg(target_arch = "wasm32")]
use crate::Action;
use crate::Event;
use flow_ngin::resources::AssetFiles;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileIntent {
    ImportFiles,
    ImportFolder,
    Open,
}
pub const MAX_BYTES: usize = 512 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
pub async fn pick(intent: FileIntent) -> Event {
    let result = async {
        let dialog = rfd::AsyncFileDialog::new();
        let paths = if intent == FileIntent::ImportFolder {
            let Some(folder) = dialog.pick_folder().await else {
                return Ok(None);
            };
            let root = folder.path().to_owned();
            let mut paths = Vec::new();
            fn visit(
                path: &std::path::Path,
                paths: &mut Vec<std::path::PathBuf>,
            ) -> anyhow::Result<()> {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let ty = entry.file_type()?;
                    if ty.is_dir() {
                        visit(&entry.path(), paths)?;
                    } else if ty.is_file() {
                        paths.push(entry.path());
                    }
                    anyhow::ensure!(paths.len() <= 10000, "Too many files in folder");
                }
                Ok(())
            }
            visit(&root, &mut paths)?;
            (root, paths)
        } else {
            let selected = if intent == FileIntent::Open {
                dialog
                    .add_filter("Schematic project", &["zip", "bin"])
                    .pick_file()
                    .await
                    .map(|f| vec![f])
            } else {
                dialog
                    .set_title("Select models and all companion files, or use Import folder")
                    .pick_files()
                    .await
            };
            let Some(selected) = selected else {
                return Ok(None);
            };
            let paths = selected
                .iter()
                .map(|f| f.path().to_owned())
                .collect::<Vec<_>>();
            let Some(first) = paths.first() else {
                return Ok(None);
            };
            let mut root = first.parent().unwrap().to_owned();
            while !paths.iter().all(|p| p.starts_with(&root)) {
                if !root.pop() {
                    break;
                }
            }
            (root, paths)
        };
        let mut files = AssetFiles::new();
        let mut total = 0usize;
        for path in paths.1 {
            let size = std::fs::metadata(&path)?.len();
            anyhow::ensure!(
                size <= MAX_BYTES as u64
                    && total
                        .checked_add(size as usize)
                        .is_some_and(|n| n <= MAX_BYTES),
                "Selection exceeds 512 MiB"
            );
            let bytes = tokio::fs::read(&path).await?;
            total += bytes.len();
            anyhow::ensure!(total <= MAX_BYTES, "Selection exceeds 512 MiB");
            let name = path
                .strip_prefix(&paths.0)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Filename is not UTF-8"))?
                .replace('\\', "/");
            anyhow::ensure!(files.insert(name, bytes).is_none(), "Duplicate filename");
        }
        Ok::<_, anyhow::Error>(Some(files))
    }
    .await;
    match result {
        Ok(Some(files)) => Event::Files(intent, files),
        Ok(None) => Event::Idle,
        Err(e) => Event::Error(format!("{e:#}")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn save(name: &str, bytes: Vec<u8>, revision: u64, project: bool) -> Event {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_file_name(name)
        .save_file()
        .await
    else {
        return Event::Idle;
    };
    match file.write(&bytes).await {
        Ok(()) => Event::Saved { revision, project },
        Err(e) => Event::Error(format!("Save failed: {e}")),
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque};
    use wasm_bindgen::{JsCast, prelude::*};
    thread_local! { static EVENTS:RefCell<VecDeque<Event>>=const { RefCell::new(VecDeque::new()) }; }
    pub fn next() -> Option<Event> {
        EVENTS.with(|events| events.borrow_mut().pop_front())
    }
    #[wasm_bindgen]
    pub fn receive_files(kind: &str, names: js_sys::Array, data: js_sys::Array) {
        let intent = if kind == "open" {
            FileIntent::Open
        } else {
            FileIntent::ImportFiles
        };
        let mut files = AssetFiles::new();
        let mut total = 0usize;
        let mut error = None;
        if names.length() != data.length() {
            error = Some("Invalid file selection".to_owned());
        }
        for i in 0..names.length() {
            let Some(name) = names.get(i).as_string() else {
                error = Some("Invalid filename".into());
                break;
            };
            let data = js_sys::Uint8Array::new(&data.get(i));
            total = total.saturating_add(data.length() as usize);
            if total > MAX_BYTES {
                error = Some("Selection exceeds 512 MiB".into());
                break;
            }
            if files.insert(name, data.to_vec()).is_some() {
                error = Some("Duplicate filenames; use Import folder".into());
                break;
            }
        }
        EVENTS.with(|events| {
            events.borrow_mut().push_back(match error {
                Some(e) => Event::Error(e),
                None => Event::Files(intent, files),
            })
        });
    }
    #[wasm_bindgen]
    pub fn file_action(action: &str) {
        let action = match action {
            "export" => Action::Export(true),
            "binary" => Action::Export(false),
            _ => return,
        };
        EVENTS.with(|events| events.borrow_mut().push_back(Event::Action(action)));
    }
    pub async fn save(name: &str, bytes: Vec<u8>, revision: u64, project: bool) -> Event {
        let result = (|| -> Result<(), JsValue> {
            let document = web_sys::window().unwrap().document().unwrap();
            let data = js_sys::Array::new();
            data.push(&js_sys::Uint8Array::from(bytes.as_slice()));
            let options = web_sys::BlobPropertyBag::new();
            options.set_type("application/octet-stream");
            let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&data, &options)?;
            let url = web_sys::Url::create_object_url_with_blob(&blob)?;
            let link = document
                .create_element("a")?
                .dyn_into::<web_sys::HtmlAnchorElement>()?;
            link.set_href(&url);
            link.set_download(name);
            document.body().unwrap().append_child(&link)?;
            link.click();
            link.remove();
            // Let the browser begin consuming the object URL before releasing it.
            let cleanup = Closure::once_into_js(move || {
                let _ = web_sys::Url::revoke_object_url(&url);
            });
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    cleanup.unchecked_ref(),
                    1000,
                )?;
            Ok(())
        })();
        match result {
            Ok(()) => Event::Saved { revision, project },
            Err(e) => Event::Error(format!("Download failed: {e:?}")),
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use web::{file_action, next, receive_files, save};
