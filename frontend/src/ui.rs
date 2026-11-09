use crate::{PageQuery, api::*};
use leptos::prelude::*;
fn shell(org: &str, tab: &str, content: impl IntoView) -> String {
    let nav = [
        ("files", "▤", "My files"),
        ("received", "↙", "Shared with me"),
        ("shared", "↗", "Shared by me"),
        ("activity", "◷", "Activity"),
    ];
    let html = view! {
        <html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/>
        <title>"Frith by Heimdex"</title><link rel="stylesheet" href="/assets/style.css"/><script src="/assets/app.js" defer></script></head>
        <body><a class="skip" href="#main">"Skip to content"</a><div class="app">
        <aside><a class="brand" href=format!("/?org={org}")><span class="brand-mark">"f"</span>"frith"<span class="byline">"by Heimdex"</span></a>
        <div class="workspace"><span class="avatar">{org_name(org).chars().next().unwrap_or('H').to_string()}</span><div><strong>{org_name(org).to_string()}</strong><small>"Private workspace"</small></div></div>
        <nav aria-label="Main navigation">{nav.into_iter().map(|(key, icon, label)| view! { <a class=if tab == key { "nav-item active" } else { "nav-item" } aria-current=if tab == key { Some("page") } else { None } href=format!("/?org={org}&tab={key}")><span aria-hidden="true">{icon}</span>{label}</a> }).collect_view()}</nav>
        <div class="sidebar-bottom"><span class="demo-dot"></span>"Mock workspace"<form method="get" action="/"><label for="org">"View as organization"</label><select id="org" name="org">{ORGS.into_iter().map(|(id, name, _)| view! { <option value=id selected=id==org>{name}</option> }).collect_view()}</select><button class="switch" type="submit">"Switch workspace →"</button></form><small>"Session data resets on restart"</small></div></aside>
        <main id="main"><div class="topbar"><span>"Workspace"</span><span>"Private exchange"<span class="top-avatar">{org_name(org).chars().next().unwrap_or('H').to_string()}</span></span></div>{content}</main></div></body></html>
    }.to_html();
    format!("<!DOCTYPE html>{html}")
}
fn badge(status: Status) -> impl IntoView {
    view! { <span class=format!("badge {}", status.class())><span class="status-dot"></span>{status.label()}</span> }
}
fn action(
    org: &str,
    d: &Dataset,
    command: &str,
    label: &str,
    clip: usize,
    primary: bool,
) -> impl IntoView {
    view! { <form method="post" action=format!("/datasets/{}/change?org={org}", d.id)><input type="hidden" name="revision" value=d.revision/><input type="hidden" name="action" value=command.to_string()/><input type="hidden" name="clip" value=clip/><button class=if primary { "button primary" } else { "button" }>{label.to_string()}</button></form> }
}
pub fn index(org: &str, query: &PageQuery, datasets: Vec<Dataset>, events: Vec<Event>) -> String {
    let tab = query.tab.as_deref().unwrap_or("files");
    let title = match tab {
        "received" => "Shared with me",
        "shared" => "Shared by me",
        "activity" => "Activity",
        _ => "My files",
    };
    let search = query.q.as_deref().unwrap_or("");
    let filtered: Vec<_> = datasets
        .into_iter()
        .filter(|d| {
            (match tab {
                "received" => d.owner != org,
                "shared" => d.owner == org && !d.grants.is_empty(),
                _ => d.owner == org,
            }) && d.name.to_lowercase().contains(&search.to_lowercase())
        })
        .collect();
    let count = filtered.len();
    shell(
        org,
        tab,
        view! { <section class="page"><header class="page-heading"><div><div class="eyebrow">"LIBRARY"</div><h1>{title}</h1></div>{(tab != "activity").then(|| view! { <a class="button primary" href=format!("/datasets/new?org={org}")>"＋ New dataset"</a> })}</header>
            {if tab == "activity" { view! { <div class="activity-list">{events.into_iter().map(|e| view! { <div class="event"><span class="event-icon">"↗"</span><div>{e.text}<small>"This demo session"</small></div></div> }).collect_view()}</div> }.into_any() } else { view! {
            <div class="toolbar"><span>{format!("{count} {}", if count == 1 { "dataset" } else { "datasets" })}</span><form class="search" method="get" action="/"><input type="hidden" name="org" value=org.to_string()/><input type="hidden" name="tab" value=tab.to_string()/><input name="q" aria-label="Search datasets" placeholder="Search datasets…" value=search.to_string()/><button type="submit" aria-label="Search">"⌕"</button></form></div>
            <div class="table-wrap"><table><thead><tr><th>"Name"</th><th>"Videos"</th><th>"Face anonymization"</th><th>{if tab == "received" { "Owner" } else { "Shared with" }}</th><th>"Processing location"</th></tr></thead><tbody>
            {filtered.into_iter().map(|d| { let sharing = if d.owner != org { org_name(&d.owner).to_string() } else if d.grants.is_empty() { "Only your organization".into() } else { d.grants.iter().map(|g| org_name(&g.org)).collect::<Vec<_>>().join(", ") }; view! {
                <tr><td><a class="dataset-link" href=format!("/datasets/{}?org={org}", d.id)><span class="folder-icon" aria-hidden="true">"▱"</span><span>{d.name}<small>"Dataset · v1"</small></span></a></td><td>{d.clips.len()}</td><td>{badge(d.status)}</td><td>{sharing}</td><td>{jurisdiction(&d.owner).to_string()}</td></tr>
            } }).collect_view()}</tbody></table></div>
            {(count == 0).then(|| view! { <div class="empty"><h2>"No datasets here yet"</h2><p>"Try another search or create a dataset."</p></div> })}
            <footer class="list-footer"><span>"◎"</span>" Organization-only access. Face anonymization stays in the sharer’s jurisdiction."</footer>
            }.into_any()}}
        </section> },
    )
}
pub fn new_dataset(org: &str) -> String {
    shell(
        org,
        "files",
        view! { <section class="page narrow"><a class="back" href=format!("/?org={org}")>"← My files"</a><header class="page-heading"><div><div class="eyebrow">"NEW DATASET"</div><h1>"Start with your videos"</h1></div></header>
        <form method="post" action=format!("/datasets?org={org}") class="create-form"><label for="name">"Dataset name"</label><input id="name" name="name" placeholder="e.g. Warehouse · morning shift" required maxlength="100" autofocus/>
        <label for="videos">"Videos"</label><div class="dropzone"><span class="upload-icon">"↑"</span><strong>"Choose videos"</strong><span>"MP4, MOV or WebM · Up to 50 files"</span><input id="videos" type="file" accept="video/mp4,video/quicktime,video/webm,.mp4,.mov,.webm" multiple aria-describedby="mock-note"/><button class="button" type="button" id="use-sample">"Use sample videos"</button></div>
        <textarea id="files" name="files" hidden required></textarea><div id="file-list" role="status" aria-live="polite"></div>
        <div class="location-card"><span>"◎"</span><div><strong>{jurisdiction(org).to_string()}</strong><small>"Face processing location · Set by your organization"</small></div></div>
        <p class="muted" id="mock-note">"Demo: only filenames are registered. No video bytes are uploaded or processed."</p><div class="form-actions"><a class="button" href=format!("/?org={org}")>"Cancel"</a><button class="button primary" id="create-button" disabled>"Create dataset"</button></div></form></section> },
    )
}
pub fn detail(org: &str, d: Dataset, clip: usize) -> String {
    let owner = d.owner == org;
    let ready = matches!(d.status, Status::Review | Status::Ready);
    let can_download =
        d.status == Status::Ready && (owner || d.grants.iter().any(|g| g.org == org && g.download));
    let tab = if owner { "files" } else { "received" };
    shell(
        org,
        tab,
        view! { <section class="page"><a class="back" href=format!("/?org={org}&tab={tab}")>{if owner { "← My files" } else { "← Shared with me" }}</a>
        <header class="page-heading"><div><div class="eyebrow">{format!("DATASET / {} / V1", org_name(&d.owner))}</div><h1>{d.name.clone()}</h1><div class="dataset-meta">{badge(d.status)}<span>{format!("{} videos", d.clips.len())}</span><span>{format!("Faces · {}", jurisdiction(&d.owner))}</span></div></div>
        <div class="actions">{can_download.then(|| view! { <a class="button" href=format!("/datasets/{}/download/{clip}?org={org}", d.id)>"↓ Download video"</a> })}{(owner && d.status == Status::Ready).then(|| view! { <button class="button primary" data-open="share-dialog">"Share dataset"</button> })}</div></header>
        <div class="detail-grid"><section class="preview-card"><div class="preview-heading"><strong>{d.clips[clip].name.clone()}</strong><span>{if ready { "Anonymized derivative" } else { "Preview pending" }}</span></div>
        {if ready && owner { comparison_player(org, &d, clip).into_any() } else if ready { view! { <video controls preload="metadata" controlslist=if can_download { "" } else { "nodownload" } aria-label="Synthetic anonymized video preview" src=format!("/datasets/{}/media/{clip}?org={org}", d.id)>"Your browser does not support video playback."</video> }.into_any() } else { view! { <div class="preview-pending"><span>"◎"</span><h2>{d.status.label()}</h2><p>{if d.status == Status::Failed { "Retry processing to create a new preview." } else { "Your preview will appear after face processing." }}</p></div> }.into_any()}}
        <div class="preview-caption"><span>"SYNTHETIC DEMO"</span>" All videos use the same synthetic sample pair. No customer video is processed."</div>
        {(owner && d.status == Status::Review).then(|| view! { <div class="review-bar"><div><strong>{if d.clips[clip].approved { "Video approved" } else { "Review face coverage" }}</strong><small>"Review the full video before approving."</small></div><div class="actions">{action(org, &d, "reject", "Needs reprocessing", clip, false)}{(!d.clips[clip].approved).then(|| action(org, &d, "approve", "Approve video", clip, true))}</div></div> })}
        {(owner && d.status == Status::Processing).then(|| view! { <div class="review-bar"><strong>"Mock processing controls"</strong><div class="actions">{action(org, &d, "fail", "Simulate failure", clip, false)}{action(org, &d, "finish", "Finish processing", clip, true)}</div></div> })}
        {(owner && d.status == Status::Failed).then(|| view! { <div class="review-bar"><span>"No videos can be shared."</span>{action(org, &d, "retry", "Retry processing", clip, true)}</div> })}
        </section><section class="files-panel"><h2>"Videos"<span>{d.clips.len()}</span></h2>{d.clips.iter().enumerate().map(|(i,c)| view! { <a class=if i == clip { "clip selected" } else { "clip" } aria-current=if i==clip { Some("true") } else { None } href=format!("/datasets/{}?org={org}&clip={i}", d.id)><span class="clip-icon">"▷"</span><div><strong>{c.name.clone()}</strong><small>{if c.approved { "Approved" } else if d.status == Status::Review { "Awaiting review" } else { d.status.label() }}</small></div></a> }).collect_view()}<div class="panel-note"><strong>{if owner { "Processing location" } else { "Your access" }}</strong><small>{if owner { jurisdiction(org).to_string() } else if can_download { "Preview + download".into() } else { "Preview only".into() }}</small></div></section></div>
        {(owner && !d.grants.is_empty()).then(|| view! { <section class="access-section"><h2>"Shared with"</h2>{d.grants.iter().map(|g| view! { <div class="access-row"><span class="avatar">{org_name(&g.org).chars().next().unwrap_or('O').to_string()}</span><strong>{org_name(&g.org).to_string()}</strong><span>{if g.download { "Preview + download" } else { "Preview only" }}</span><form method="post" action=format!("/datasets/{}/change?org={org}", d.id)><input type="hidden" name="revision" value=d.revision/><input type="hidden" name="action" value="revoke"/><input type="hidden" name="recipient" value=g.org.clone()/><button class="text-button">"Revoke access"</button></form></div> }).collect_view()}</section> })}
        {(owner && d.status == Status::Ready).then(|| share_dialog(org, &d))}
        </section> },
    )
}
fn share_dialog(org: &str, d: &Dataset) -> impl IntoView {
    view! { <dialog id="share-dialog" aria-labelledby="share-title"><div class="dialog-heading"><h2 id="share-title">"Share dataset"</h2><button class="icon-button" data-close="share-dialog" aria-label="Close dialog">"×"</button></div><p class="dialog-subtitle">{d.name.clone()}</p>
    <form method="post" action=format!("/datasets/{}/change?org={org}", d.id)><input type="hidden" name="revision" value=d.revision/><input type="hidden" name="action" value="share"/>
    <label for="recipient">"Organization"</label><select id="recipient" name="recipient" required><option value="" disabled selected>"Select an invited organization"</option>{ORGS.into_iter().filter(|o| o.0 != org).map(|(id,name,_)| view! { <option value=id>{name}</option> }).collect_view()}</select>
    <fieldset><legend>"Access"</legend><label class="permission"><input type="radio" name="permission" value="preview" checked/><div><strong>"Preview only"</strong><small>"Watch approved videos"</small></div></label><label class="permission"><input type="radio" name="permission" value="download"/><div><strong>"Preview + download"</strong><small>"Watch and download approved videos"</small></div></label></fieldset>
    <p class="muted">"Only approved face-anonymized videos are shared."</p><div class="form-actions"><button class="button" type="button" data-close="share-dialog">"Cancel"</button><button class="button primary">"Save access"</button></div></form></dialog> }
}
pub fn error(message: &str) -> String {
    shell(
        "heimdex",
        "",
        view! { <section class="page narrow"><div class="eyebrow">"ACTION NOT COMPLETED"</div><h1>"Let’s try that again"</h1><p role="alert">{message.to_string()}</p><button class="button" id="go-back">"← Go back"</button></section> },
    )
}

fn comparison_player(org: &str, d: &Dataset, clip: usize) -> impl IntoView {
    view! {
        <section class="comparison-player" aria-label="Face review player" data-comparison>
            <div class="compare-toolbar"><label><input type="checkbox" data-compare-toggle/>"Compare original"</label><span>"Owner review only"</span></div>
            <div class="compare-stage" style="--split:50%">
                <video class="processed-video" controls playsinline preload="metadata" aria-label="Face anonymized video" src=format!("/datasets/{}/media/{clip}?org={org}", d.id)></video>
                <video class="original-video" playsinline muted preload="none" aria-hidden="true" tabindex="-1" data-source=format!("/datasets/{}/original/{clip}?org={org}", d.id)></video>
                <div class="compare-labels" hidden><span>"Original"</span><span>"Face anonymized"</span></div>
                <div class="compare-divider" hidden role="slider" tabindex="0" aria-label="Original reveal" aria-valuemin="0" aria-valuemax="100" aria-valuenow="50" aria-valuetext="50% original" aria-orientation="horizontal"><span>"↔"</span></div>
                <div class="compare-shield" hidden>"Aligning video pair…"</div>
            </div>
            <div class="player-controls" hidden>
                <button type="button" data-play aria-label="Play">"Play"</button>
                <input type="range" data-seek min="0" max="8" step="0.01" value="0" aria-label="Video position"/>
                <output data-time>"0:00 / 0:08"</output>
                <select data-speed aria-label="Playback speed"><option value="0.5">"0.5×"</option><option value="1" selected>"1×"</option><option value="1.5">"1.5×"</option><option value="2">"2×"</option></select>
                <input type="range" data-volume min="0" max="1" step="0.05" value="1" aria-label="Volume"/>
                <button type="button" data-fullscreen>"Fullscreen"</button>
            </div>
            <p class="player-status" role="status" aria-live="polite"></p>
            <script src="/assets/comparison.js" defer></script>
        </section>
    }
}
