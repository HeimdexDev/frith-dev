use crate::{PackageStage, PageQuery, WorkspaceView, api::*};
use leptos::prelude::*;
pub(crate) fn shell(org: &str, mode: WorkspaceView, tab: &str, content: impl IntoView) -> String {
    let nav = [
        ("requests", "◎", "Data requests"),
        ("files", "▤", "Safe sharing"),
        ("activity", "◷", "Activity"),
    ];
    let tab = if matches!(tab, "received" | "shared") {
        "files"
    } else {
        tab
    };
    let mode_key = mode.key();
    let html = view! {
        <html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/>
        <title>"Frith by Heimdex"</title><link rel="stylesheet" href="/assets/style.css"/><script src="/assets/app.js" defer></script></head>
        <body><a class="skip" href="#main">"Skip to content"</a><div class="app">
        <aside><a class="brand" href=format!("/?org={org}&view={mode_key}")><span class="brand-mark">"f"</span>"frith"<span class="byline">"by Heimdex"</span></a>
        <div class="workspace"><span class="avatar">{org_name(org).chars().next().unwrap_or('H').to_string()}</span><div><strong>{org_name(org).to_string()}</strong><small>"Private workspace"</small></div></div>
        <div class="view-switcher"><span class="view-label">"Workspace view"</span><div class="view-options" role="group" aria-label="Buying or supplying">{[WorkspaceView::Buying, WorkspaceView::Supplying].into_iter().map(|view| view! { <a href=format!("/?org={org}&tab={tab}&view={}",view.key()) class=if mode == view { "selected" } else { "" } aria-current=if mode==view {Some("true")}else{None}>{view.label()}</a> }).collect_view()}</div><small>{if mode==WorkspaceView::Buying {"Find and receive data"}else{"Respond, prepare and share"}}</small></div>
        <nav aria-label="Main navigation">{nav.into_iter().map(|(key, icon, label)| view! { <a class=if tab == key { "nav-item active" } else { "nav-item" } aria-current=if tab == key { Some("page") } else { None } href=format!("/?org={org}&tab={key}&view={mode_key}")><span aria-hidden="true">{icon}</span>{label}</a> }).collect_view()}</nav>
        <div class="sidebar-bottom"><span class="demo-dot"></span>"Mock workspace"<form method="get" action="/"><input type="hidden" name="view" value=mode_key/><label for="org">"View as organization"</label><select id="org" name="org">{ORGS.into_iter().map(|(id, name, _)| view! { <option value=id selected=id==org>{name}</option> }).collect_view()}</select><button class="switch" type="submit">"Switch workspace →"</button></form><small>"Session data resets on restart"</small></div></aside>
        <main id="main"><div class="topbar"><span>{format!("{} / {}",org_name(org),mode.label())}</span><span>"Private exchange"<span class="top-avatar">{org_name(org).chars().next().unwrap_or('H').to_string()}</span></span></div>{content}</main></div></body></html>
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
    let mode = query.workspace_view(org);
    let buying = mode == WorkspaceView::Buying;
    let mode_key = mode.key();
    let activity = query.tab.as_deref() == Some("activity");
    let stage = query
        .stage
        .unwrap_or(if query.tab.as_deref() == Some("shared") {
            PackageStage::Shared
        } else {
            PackageStage::All
        });
    let search = query.q.as_deref().unwrap_or("");
    let filtered: Vec<_> = datasets
        .into_iter()
        .filter(|d| {
            (if buying {
                d.owner != org
            } else {
                d.owner == org
            }) && d.name.to_lowercase().contains(&search.to_lowercase())
                && (buying
                    || match stage {
                        PackageStage::All => true,
                        PackageStage::Processing => d.status == Status::Processing,
                        PackageStage::Review => d.status == Status::Review,
                        PackageStage::Ready => d.status == Status::Ready,
                        PackageStage::Failed => d.status == Status::Failed,
                        PackageStage::Shared => !d.grants.is_empty(),
                    })
        })
        .collect();
    let count = filtered.len();
    shell(
        org,
        mode,
        if activity { "activity" } else { "files" },
        view! {
            <section class="page"><header class="page-heading"><div><div class="eyebrow">{if activity {"AUDIT TRAIL"}else{"SAFE SHARING"}}</div><h1>{if activity {"Activity"}else if buying {"Received datasets"}else{"Outgoing packages"}}</h1><p class="page-intro">{if activity {"Organization activity includes buying and supplying. Only your organization’s events are shown."}else if buying {"Access datasets that suppliers have explicitly shared with your organization."}else{"Prepare datasets, review processed videos, and manage recipient access."}}</p></div>{(!buying && !activity).then(||view!{<a class="button primary" href=format!("/datasets/new?org={org}")>"＋ New sharing package"</a>})}</header>
            {if activity {view!{<div class="activity-list">{events.into_iter().map(|e|view!{<div class="event"><span class="event-icon">"↗"</span><div>{e.text}<small>"This demo session"</small></div></div>}).collect_view()}</div>}.into_any()}else{view!{
                {(!buying).then(||view!{<div class="section-tabs" aria-label="Package workflow">{[(PackageStage::All,"All packages"),(PackageStage::Processing,"Processing"),(PackageStage::Review,"Needs review"),(PackageStage::Failed,"Needs attention"),(PackageStage::Ready,"Ready to share"),(PackageStage::Shared,"Active access")].into_iter().map(|(key,label)|view!{<a class=if stage==key {"selected"}else{""} aria-current=if stage==key{Some("page")}else{None} href=format!("/?org={org}&tab=files&view=supplying&stage={}",key.key())>{label}</a>}).collect_view()}</div>})}
                <div class="toolbar"><span>{format!("{count} {}",if count==1{"dataset"}else{"datasets"})}</span><form class="search" method="get" action="/"><input type="hidden" name="org" value=org.to_string()/><input type="hidden" name="tab" value="files"/><input type="hidden" name="view" value=mode_key/><input type="hidden" name="stage" value=stage.key()/><input name="q" aria-label="Search datasets" placeholder="Search datasets…" value=search.to_string()/><button type="submit" aria-label="Search">"⌕"</button></form></div>
                <div class="table-wrap"><table><thead><tr><th>"Name"</th><th>"Videos"</th><th>{if buying {"Your access"}else{"Face review"}}</th><th>{if buying {"Supplier"}else{"Shared with"}}</th><th>"Processing location"</th></tr></thead><tbody>{filtered.into_iter().map(|d|{
                    let sharing=if buying{org_name(&d.owner).to_string()}else if d.grants.is_empty(){"No recipient access".into()}else{d.grants.iter().map(|g|org_name(&g.org)).collect::<Vec<_>>().join(", ")};
                    let access=if d.grants.iter().any(|g|g.org==org&&g.download){"Preview + download"}else{"Preview only"};
                    view!{<tr><td><a class="dataset-link" href=format!("/datasets/{}?org={org}",d.id)><span class="folder-icon" aria-hidden="true">"▱"</span><span>{d.name}<small>"Package · v1"</small></span></a></td><td>{d.clips.len()}</td><td>{if buying{view!{<span class="badge">{access}</span>}.into_any()}else{badge(d.status).into_any()}}</td><td>{sharing}</td><td>{jurisdiction(&d.owner).to_string()}</td></tr>}
                }).collect_view()}</tbody></table></div>
                {(count==0).then(||view!{<div class="empty"><h2>"No datasets here yet"</h2><p>{if buying{"A supplier must complete review and grant access before a dataset appears here."}else{"Try another filter or create a sharing package."}}</p></div>})}
                <footer class="list-footer">{if buying{"Request agreement does not grant access. Revocation blocks future authorized retrieval, not copies already received."}else{"Face review is separate from commercial and export approval. Processing is simulated in this demo."}}</footer>
            }.into_any()}}
            </section>
        },
    )
}
pub fn new_dataset(org: &str) -> String {
    shell(
        org,
        WorkspaceView::Supplying,
        "files",
        view! { <section class="page narrow"><a class="back" href=format!("/?org={org}&tab=files&view=supplying")>"← Safe sharing"</a><header class="page-heading"><div><div class="eyebrow">"NEW SHARING PACKAGE"</div><h1>"Start with your videos"</h1></div></header>
        <form method="post" action=format!("/datasets?org={org}") class="create-form"><label for="name">"Dataset name"</label><input id="name" name="name" placeholder="e.g. Warehouse · morning shift" required maxlength="100" autofocus/>
        <label for="videos">"Videos"</label><div class="dropzone"><span class="upload-icon">"↑"</span><strong>"Choose videos"</strong><span>"MP4, MOV or WebM · Up to 50 files"</span><input id="videos" type="file" accept="video/mp4,video/quicktime,video/webm,.mp4,.mov,.webm" multiple aria-describedby="mock-note"/><button class="button" type="button" id="use-sample">"Use sample videos"</button></div>
        <textarea id="files" name="files" hidden required></textarea><div id="file-list" role="status" aria-live="polite"></div>
        <div class="location-card"><span>"◎"</span><div><strong>{jurisdiction(org).to_string()}</strong><small>"Face processing location · Set by your organization"</small></div></div>
        <p class="muted" id="mock-note">"Demo: only filenames are registered. No video bytes are uploaded or processed."</p><div class="form-actions"><a class="button" href=format!("/?org={org}&tab=files&view=supplying")>"Cancel"</a><button class="button primary" id="create-button" disabled>"Create sharing package"</button></div></form></section> },
    )
}
pub fn detail(
    org: &str,
    d: Dataset,
    clip: usize,
    gpu_enabled: bool,
    running: bool,
    live_job: bool,
) -> String {
    let owner = d.owner == org;
    let ready = matches!(d.status, Status::Review | Status::Ready);
    let can_download =
        d.status == Status::Ready && (owner || d.grants.iter().any(|g| g.org == org && g.download));
    let tab = "files";
    let mode = if owner {
        WorkspaceView::Supplying
    } else {
        WorkspaceView::Buying
    };
    let mode_key = mode.key();
    shell(
        org,
        mode,
        tab,
        view! { <section class="page">{running.then(||view!{<meta http-equiv="refresh" content="3"/>})}<a class="back" href=format!("/?org={org}&tab={tab}&view={mode_key}")>{if owner { "← Outgoing packages" } else { "← Received datasets" }}</a>
        <header class="page-heading"><div><div class="eyebrow">{format!("DATASET / {} / V1", org_name(&d.owner))}</div><h1>{d.name.clone()}</h1><div class="dataset-meta">{badge(d.status)}<span>{format!("{} videos", d.clips.len())}</span><span>{format!("Faces · {}", jurisdiction(&d.owner))}</span></div></div>
        <div class="actions">{can_download.then(|| view! { <a class="button" href=format!("/datasets/{}/download/{clip}?org={org}", d.id)>"↓ Download video"</a> })}{(owner && d.status == Status::Ready).then(|| view! { <button class="button primary" data-open="share-dialog">"Share dataset"</button> })}</div></header>
        {d.request_origin.as_ref().map(|origin| view! { <div class="exchange-note"><a class="request-link" href=format!("/requests/{}?org={org}", origin.request_id)>{format!("Request #{} · Package v{}", origin.request_id, origin.version)}</a><p>{format!("Buyer: {} · Purpose: {} · Destination: {}", org_name(&origin.buyer), origin.requirements.purpose, origin.requirements.destination)}</p><small>"Agreed scope is frozen for this package. Access still requires completed review and a separate grant."</small></div> })}
        {owner.then(|| view! { <p class="exchange-note">"Face review prepares a dataset for sharing in this demo. Production delivery also requires rights clearance, an agreed purpose and recipient, and export authorization."</p> })}
        {owner.then(|| view! { <ol class="package-steps" aria-label="Preparation workflow"><li aria-current=if matches!(d.status,Status::Processing|Status::Failed){Some("step")}else{None}><strong>"1. Process locally"</strong><small>{if d.status==Status::Failed{"Failed · Retry needed"}else if d.status==Status::Processing{if running {"GPU job running · This page refreshes automatically"} else if gpu_enabled {"Ready to process the configured demo sample"} else {"Mock processing pending"}}else{if live_job {"GPU output received"} else {"Mock processing complete"}}}</small></li><li aria-current=if d.status==Status::Review{Some("step")}else{None}><strong>"2. Review faces"</strong><small>{if d.status==Status::Ready{"All videos reviewed"}else{"Approve every video or return for processing"}}</small></li><li aria-current=if d.status==Status::Ready{Some("step")}else{None}><strong>"3. Manage access"</strong><small>{if d.grants.is_empty(){"No recipient access granted"}else{"Recipient grants active"}}</small></li></ol> })}
        <div class="detail-grid"><section class="preview-card"><div class="preview-heading"><strong>{d.clips[clip].name.clone()}</strong><span>{if ready { "Anonymized derivative" } else { "Preview pending" }}</span></div>
        {if ready && owner { comparison_player(org, &d, clip).into_any() } else if ready { view! { <video controls preload="metadata" controlslist=if can_download { "" } else { "nodownload" } aria-label="Synthetic anonymized video preview" src=format!("/datasets/{}/media/{clip}?org={org}", d.id)>"Your browser does not support video playback."</video> }.into_any() } else { view! { <div class="preview-pending"><span>"◎"</span><h2>{d.status.label()}</h2><p>{if d.status == Status::Failed { if live_job {"GPU job failed or timed out. Check the worker and AWS access, then retry."} else {"Retry processing to create a new preview."} } else { "Your preview will appear after face processing." }}</p></div> }.into_any()}}
        <div class="preview-caption"><span>{if live_job {"GPU DEMO"} else {"SYNTHETIC DEMO"}}</span>{if live_job {" The configured sample is processed once for this package; all listed clips show that same result."} else {" All videos use the same synthetic sample pair. No customer video is processed."}}</div>
        {(owner && d.status == Status::Review).then(|| view! { <div class="review-bar"><div><strong>{if d.clips[clip].approved { "Video approved" } else { "Review face coverage" }}</strong><small>"Review the full video before approving."</small></div><div class="actions">{action(org, &d, "reject", "Needs reprocessing", clip, false)}{(!d.clips[clip].approved).then(|| action(org, &d, "approve", "Approve video", clip, true))}</div></div> })}
        {(owner && d.status == Status::Processing).then(|| if gpu_enabled {view!{<div class="review-bar"><strong>{if running {"Processing on GPU…"} else {"Process the configured demo sample"}}</strong><p>"Uses the existing AWS queue and GPU worker. Allow about a minute for a short clip; timeout is five minutes."</p>{(!running).then(||action(org,&d,"process","Process on GPU",clip,true))}</div>}.into_any()}else{view!{<div class="review-bar"><strong>"Mock processing controls"</strong><div class="actions">{action(org, &d, "fail", "Simulate failure", clip, false)}{action(org, &d, "finish", "Finish processing", clip, true)}</div></div>}.into_any()})}
        {(owner && d.status == Status::Failed).then(|| view! { <div class="review-bar"><span>"No videos can be shared."</span>{action(org, &d, "retry", "Retry processing", clip, true)}</div> })}
        </section><section class="files-panel"><h2>"Videos"<span>{d.clips.len()}</span></h2>{d.clips.iter().enumerate().map(|(i,c)| view! { <a class=if i == clip { "clip selected" } else { "clip" } aria-current=if i==clip { Some("true") } else { None } href=format!("/datasets/{}?org={org}&clip={i}", d.id)><span class="clip-icon">"▷"</span><div><strong>{c.name.clone()}</strong><small>{if c.approved { "Approved" } else if d.status == Status::Review { "Awaiting review" } else { d.status.label() }}</small></div></a> }).collect_view()}<div class="panel-note"><strong>{if owner { "Processing location" } else { "Your access" }}</strong><small>{if owner { jurisdiction(org).to_string() } else if can_download { "Preview + download".into() } else { "Preview only".into() }}</small></div></section></div>
        {(owner && !d.grants.is_empty()).then(|| view! { <section class="access-section"><h2>"Shared with"</h2>{d.grants.iter().map(|g| view! { <div class="access-row"><span class="avatar">{org_name(&g.org).chars().next().unwrap_or('O').to_string()}</span><strong>{org_name(&g.org).to_string()}</strong><span>{if g.download { "Preview + download" } else { "Preview only" }}</span><form method="post" action=format!("/datasets/{}/change?org={org}", d.id)><input type="hidden" name="revision" value=d.revision/><input type="hidden" name="action" value="revoke"/><input type="hidden" name="recipient" value=g.org.clone()/><button class="text-button">"Revoke access"</button></form></div> }).collect_view()}</section> })}
        {(owner && d.status == Status::Ready).then(|| share_dialog(org, &d))}
        </section> },
    )
}
fn share_dialog(org: &str, d: &Dataset) -> impl IntoView {
    view! { <dialog id="share-dialog" aria-labelledby="share-title"><div class="dialog-heading"><h2 id="share-title">"Share dataset"</h2><button class="icon-button" data-close="share-dialog" aria-label="Close dialog">"×"</button></div><p class="dialog-subtitle">{d.name.clone()}</p>
    {d.request_origin.as_ref().map(|origin|view!{<div class="access-context"><strong>"Agreed request scope"</strong><dl><dt>"Recipient organization"</dt><dd>{org_name(&origin.buyer).to_string()}</dd><dt>"Declared destination"</dt><dd>{origin.requirements.destination.clone()}</dd><dt>"Intended use"</dt><dd>{origin.requirements.purpose.clone()}</dd></dl><small>"Buyer-declared details; not verified identity, location or export approval."</small></div>})}
    <form method="post" action=format!("/datasets/{}/change?org={org}", d.id)><input type="hidden" name="revision" value=d.revision/><input type="hidden" name="action" value="share"/>
    <label for="recipient">"Organization"</label><select id="recipient" name="recipient" required><option value="" disabled selected>"Select an invited organization"</option>{ORGS.into_iter().filter(|o| o.0 != org && d.request_origin.as_ref().is_none_or(|r| r.buyer == o.0)).map(|(id,name,_)| view! { <option value=id>{name}</option> }).collect_view()}</select>
    <fieldset><legend>"Access"</legend><label class="permission"><input type="radio" name="permission" value="preview" checked/><div><strong>"Preview only"</strong><small>"Watch approved videos"</small></div></label><label class="permission"><input type="radio" name="permission" value="download"/><div><strong>"Preview + download"</strong><small>"Watch and download approved videos"</small></div></label></fieldset>
    <p class="muted">"Demo access to reviewed synthetic videos. This does not authorize a production export."</p><div class="form-actions"><button class="button" type="button" data-close="share-dialog">"Cancel"</button><button class="button primary">"Save access"</button></div></form></dialog> }
}
pub fn error(message: &str) -> String {
    // Errors lack authenticated organization context; do not impersonate a workspace.
    let html=view! { <html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1"/><title>"Frith · Action not completed"</title><link rel="stylesheet" href="/assets/style.css"/><script src="/assets/app.js" defer></script></head><body><main class="page narrow"><div class="eyebrow">"ACTION NOT COMPLETED"</div><h1>"Let’s try that again"</h1><p role="alert">{message.to_string()}</p><button class="button" id="go-back">"← Go back"</button></main></body></html> }.to_html();
    format!("<!DOCTYPE html>{html}")
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
