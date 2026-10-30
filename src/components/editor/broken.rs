//! Links that lead nowhere, marked in the editor: a page that doesn't exist, a
//! missing file, an image that can't be loaded.

use std::collections::HashSet;

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::images::{image_src, ImageSizes};
use crate::{highlight, images, invoke, links, state::AppCtx};
use orgamnesia_core::names::page_key;

/// What the editor of a page knows of the targets of its links.
#[derive(Clone, Copy)]
pub struct BrokenLinks {
    ctx: AppCtx,
    /// The page, whose folder file links start from.
    page: StoredValue<String>,
    /// Keys of the pages of the project.
    page_keys: Memo<HashSet<String>>,
    /// The files the page links to that are missing (asked to the backend).
    missing: RwSignal<HashSet<String>>,
    sizes: RwSignal<ImageSizes>,
}

impl BrokenLinks {
    pub fn new(ctx: AppCtx, page: String, content: RwSignal<String>, sizes: RwSignal<ImageSizes>) -> Self {
        let page = StoredValue::new(page);
        let page_keys = Memo::new(move |_| {
            let loose = ctx.pref(|p| p.case_insensitive_links);
            ctx.project.files.with(|fs| fs.iter().map(|f| page_key(f.name.trim(), loose)).collect())
        });
        // The files the page links to
        let file_links = Memo::new(move |_| {
            if !ctx.pref(|p| p.mark_broken_links) { return Vec::new(); }
            let page = page.get_value();
            let mut paths: Vec<String> = content.with(|c| links::targets(c).iter()
                .filter_map(|t| links::file_target(t))
                .map(|t| images::resolve(&page, t))
                .collect());
            paths.sort();
            paths.dedup();
            paths
        });
        let missing = RwSignal::new(HashSet::new());
        let seq = StoredValue::new(0u32);
        Effect::new(move |_| {
            let paths = file_links.get();
            // Also after every write: files may have been added since
            ctx.project.links_version.track();
            let n = seq.get_value() + 1;
            seq.set_value(n);
            spawn_local(async move {
                let found = if paths.is_empty() { Ok(Vec::new()) } else { invoke::missing_files(&paths).await };
                // Only the answer to the last question counts
                if let Ok(m) = found && seq.try_get_value() == Some(n) {
                    missing.set(m.into_iter().collect());
                }
            });
        });
        Self { ctx, page, page_keys, missing, sizes }
    }

    /// Whether a link target leads nowhere; `None` when broken links aren't marked.
    pub fn check(self) -> Option<highlight::Broken> {
        let ctx = self.ctx;
        if !ctx.pref(|p| p.mark_broken_links) { return None; }
        let loose = ctx.pref(|p| p.case_insensitive_links);
        let page = self.page.get_value();
        Some(Box::new(move |target: &str| {
            if let Some(file) = links::file_target(target) {
                let path = images::resolve(&page, file);
                // A missing file, or an image that can't be loaded
                self.missing.with(|m| m.contains(&path))
                    || (images::is_image(file) && self.sizes.with(|s| s.get(&image_src(&page, file)) == Some(&None)))
            } else if links::is_page_target(target) {
                let page = orgamnesia_core::links::split(target).0;
                !self.page_keys.with(|k| k.contains(&page_key(page, loose)))
            } else {
                false
            }
        }))
    }
}
