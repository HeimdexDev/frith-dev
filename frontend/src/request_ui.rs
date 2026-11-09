use crate::{PageQuery, WorkspaceView, api::*, ui::shell};
use leptos::prelude::*;

pub fn list(org: &str, query: &PageQuery, requests: Vec<DataRequest>) -> String {
    let mode = query.workspace_view(org);
    let buying = mode == WorkspaceView::Buying;
    let mode_key = mode.key();
    let search = query.q.as_deref().unwrap_or("");
    let requests: Vec<_> = requests
        .into_iter()
        .filter(|r| {
            (if buying {
                r.buyer == org
            } else {
                r.requirements.seller == org
            }) && r
                .requirements
                .title
                .to_lowercase()
                .contains(&search.to_lowercase())
        })
        .collect();
    let empty = requests.is_empty();
    let steps = if buying {
        [
            (
                "1. Define requirements",
                "Describe the task, quality and intended use.",
            ),
            (
                "2. Review proposals",
                "Check the supplier’s footage and usage terms.",
            ),
            (
                "3. Receive access",
                "After agreement, wait for a separate access grant.",
            ),
        ]
    } else {
        [
            (
                "1. Assess incoming requests",
                "Review the buyer’s task and acceptance criteria.",
            ),
            (
                "2. Propose footage",
                "Explain fit, shortfalls and permitted use.",
            ),
            (
                "3. Prepare and share",
                "After buyer agreement, process, review and grant access.",
            ),
        ]
    };
    shell(
        org,
        mode,
        "requests",
        view! {
            <section class="page"><header class="page-heading"><div><div class="eyebrow">"DATA REQUESTS"</div><h1>{if buying{"My requests"}else{"Incoming requests"}}</h1><p class="page-intro">{if buying{"Describe the data you need and review proposals from suppliers."}else{"Review what buyers need and propose footage from your inventory."}}</p></div>{buying.then(||view!{<a class="button primary" href=format!("/requests/new?org={org}")>"＋ New data request"</a>})}</header>
            <div class="request-overview">{steps.into_iter().map(|(title,description)|view!{<div><strong>{title}</strong><p>{description}</p></div>}).collect_view()}</div>
            <div class="toolbar"><span>{format!("{} {}",requests.len(),if requests.len()==1{"request"}else{"requests"})}</span><form class="search" method="get" action="/requests"><input type="hidden" name="org" value=org.to_string()/><input type="hidden" name="view" value=mode_key/><input name="q" aria-label="Search requests" placeholder="Search requests…" value=search.to_string()/><button type="submit" aria-label="Search">"⌕"</button></form></div>
            <div class="table-wrap"><table><thead><tr><th>"Request / Task"</th><th>{if buying{"Supplier"}else{"Buyer"}}</th><th>"Requested volume"</th><th>"Status / Next step"</th><th>"Action"</th></tr></thead><tbody>{requests.into_iter().map(|r|view!{
                <tr><td><a class="dataset-link" href=format!("/requests/{}?org={org}",r.id)><span class="folder-icon">"◎"</span><span>{r.requirements.title.clone()}<small>{r.requirements.task.clone()}</small></span></a></td><td>{org_name(if buying{&r.requirements.seller}else{&r.buyer}).to_string()}</td><td>{r.requirements.volume.clone()}</td><td><span class="badge">{r.status()}</span><small>{next_step(&r,buying)}</small></td><td><a class="button" href=format!("/requests/{}?org={org}",r.id)>{if buying{"View request"}else{"Review request"}}</a></td></tr>
            }).collect_view()}</tbody></table></div>
            {empty.then(||view!{<div class="empty"><h2>{if buying{"No buying requests found"}else{"No incoming requests found"}}</h2><p>{if buying{"Create a data request or try another search."}else{"Requests appear here when a buyer selects your organization as the supplier."}}</p>{buying.then(||view!{<a class="button" href=format!("/requests/new?org={org}")>"Create your first request"</a>})}</div>})}
            <p class="muted">"Local demo · Seller proposals simulate matching. Heimdex search is not connected. No access is granted by a request."</p></section>
        },
    )
}
fn next_step(r: &DataRequest, buying: bool) -> &'static str {
    match (buying, r.package_id.is_some(), r.agreed, r.offer.is_some()) {
        (true, true, _, _) => "Check delivery and access status",
        (false, true, _, _) => "Review package and manage access",
        (true, false, true, _) => "Waiting for supplier preparation",
        (false, false, true, _) => "Prepare for sharing",
        (true, false, false, true) => "Review and agree to proposed scope",
        (false, false, false, true) => "Waiting for buyer agreement",
        (true, false, false, false) => "Waiting for supplier proposal",
        _ => "Assess requirements and propose footage",
    }
}
fn field(
    id: &'static str,
    label: &'static str,
    value: String,
    required: bool,
    placeholder: &'static str,
) -> impl IntoView {
    view! { <div class="request-field"><label for=id>{label}</label><input id=id name=id value=value required=required maxlength=if id == "title" {100} else {2000} placeholder=placeholder/></div> }
}
pub fn form(
    org: &str,
    edit: Option<(u64, u64)>,
    input: Requirements,
    error: Option<&str>,
) -> String {
    let advanced =
        !input.perspective.is_empty() || !input.deadline.is_empty() || input.action_data != "any";
    let target = edit
        .map(|(id, _)| format!("/requests/{id}/edit?org={org}"))
        .unwrap_or_else(|| format!("/requests?org={org}&view=buying"));
    let cancel = edit
        .map(|(id, _)| format!("/requests/{id}?org={org}"))
        .unwrap_or_else(|| format!("/requests?org={org}&view=buying"));
    shell(
        org,
        WorkspaceView::Buying,
        "requests",
        view! {
            <section class="page request-form-page"><a class="back" href=cancel.clone()>"← Data requests"</a><header class="page-heading"><div><div class="eyebrow">"BUYER REQUIREMENTS"</div><h1>{if edit.is_some(){"Edit requirements"}else{"New data request"}}</h1><p class="page-intro">{format!("Requesting as {}. What should the data help you do?",org_name(org))}</p></div></header>
            {error.map(|e| view! { <p class="form-error" role="alert">{e.to_string()}</p> })}
            <form method="post" action=target class="request-form">
                {edit.map(|(_,revision)| view! { <input type="hidden" name="revision" value=revision/> })}
                {field("title","Request title",input.title,true,"e.g. Pick and place into a box")}
                <div class="request-field"><label for="task">"Task description"</label><textarea id="task" name="task" required maxlength="2000" rows="3" placeholder="Describe the action, objects and environment.">{input.task}</textarea></div>
                <div class="request-field"><label for="criteria">"Acceptance criteria"</label><textarea id="criteria" name="criteria" required maxlength="2000" rows="3" placeholder="e.g. Hands and object visible, full action captured, outcome observable.">{input.criteria}</textarea></div>
                <div class="request-form-grid">{field("volume","Requested volume",input.volume,true,"e.g. 20 clips, 5–15 seconds each")}
                <div class="request-field"><label for="seller">"Invited seller"</label><select id="seller" name="seller" required><option value="" selected=input.seller.is_empty()>"Choose an organization"</option>{ORGS.into_iter().filter(|o|o.0!=org).map(|(id,name,_)|view!{<option value=id selected=input.seller==id>{name}</option>}).collect_view()}</select></div>
                {field("purpose","Intended use",input.purpose,true,"e.g. Robotics training and evaluation")}
                {field("destination","Delivery destination",input.destination,true,"e.g. Germany")}</div>
                <details open=advanced><summary>"Additional data requirements (optional)"</summary><div class="request-form-grid">
                    {field("perspective","Camera perspective",input.perspective,false,"e.g. Fixed side view")}
                    <div class="request-field"><label for="action_data">"Robot action data"</label><select id="action_data" name="action_data">{[("any","No preference"),("measured","Measured action data required"),("estimated","Estimated action data acceptable"),("none","Video only; no action data required")].into_iter().map(|(id,label)|view!{<option value=id selected=input.action_data==id>{label}</option>}).collect_view()}</select></div>
                    {field("deadline","Deadline",input.deadline,false,"e.g. End of November, or flexible")}
                </div></details>
                <p class="muted">{if edit.is_some(){"Saving changes clears the previous proposal and agreement. The seller must propose footage again."}else{"Your invited seller can see this request. It does not give either organization access to videos."}}</p>
                <div class="form-actions"><a class="button" href=cancel>"Cancel"</a><button class="button primary" type="submit">{if edit.is_some(){"Save requirements"}else{"Create data request"}}</button></div>
            </form></section>
        },
    )
}
fn command(org: &str, r: &DataRequest, action: &'static str, label: &'static str) -> impl IntoView {
    view! {<form method="post" action=format!("/requests/{}/change?org={org}",r.id)><input type="hidden" name="revision" value=r.revision/><input type="hidden" name="action" value=action/><button class="button primary">{label}</button></form>}
}
pub fn detail(org: &str, r: DataRequest, sources: Vec<Dataset>, package_visible: bool) -> String {
    let buyer = r.buyer == org;
    let mode = if buyer {
        WorkspaceView::Buying
    } else {
        WorkspaceView::Supplying
    };
    let mode_key = mode.key();
    let prepared = r.package_id.is_some();
    let requirements = &r.requirements;
    shell(
        org,
        mode,
        "requests",
        view! {
            <section class="page"><a class="back" href=format!("/requests?org={org}&view={mode_key}")>{if buyer{"← My requests"}else{"← Incoming requests"}}</a>
                <header class="page-heading"><div><div class="eyebrow">{format!("REQUEST #{} · {} → {}",r.id,org_name(&r.buyer),org_name(&requirements.seller))}</div><h1>{requirements.title.clone()}</h1><div class="dataset-meta"><span class="badge">{r.status()}</span><span>{next_step(&r,buyer)}</span></div></div>{(buyer && !prepared).then(||view!{<a class="button" href=format!("/requests/{}/edit?org={org}",r.id)>"Edit requirements"</a>})}</header>
                <div class="request-detail-grid">
                    <section class="request-card"><div class="eyebrow">"01 / REQUIREMENTS"</div><h2>"What the buyer needs"</h2><dl class="request-facts">
                        <dt>"Task"</dt><dd>{requirements.task.clone()}</dd><dt>"Acceptance criteria"</dt><dd>{requirements.criteria.clone()}</dd>
                        <dt>"Requested volume"</dt><dd>{requirements.volume.clone()}</dd><dt>"Intended use"</dt><dd>{requirements.purpose.clone()}</dd>
                        <dt>"Delivery destination"</dt><dd>{requirements.destination.clone()}</dd><dt>"Camera perspective"</dt><dd>{if requirements.perspective.is_empty(){"Not specified".into()}else{requirements.perspective.clone()}}</dd>
                        <dt>"Robot action data"</dt><dd>{match requirements.action_data.as_str(){"measured"=>"Measured required","estimated"=>"Estimated acceptable","none"=>"Video only",_=>"No preference"}}</dd>
                        <dt>"Deadline"</dt><dd>{if requirements.deadline.is_empty(){"Not specified".into()}else{requirements.deadline.clone()}}</dd>
                    </dl></section>
                    <div class="request-detail-stack"><section class="request-card"><div class="eyebrow">"02 / MATCHES"</div><h2>"Proposed footage"</h2><p class="muted">"Simulated matching · The seller selects a whole dataset. No automatic search or clip-level matching is connected."</p>
                        {r.offer.as_ref().map(|offer|view!{<div class="offer-summary"><strong>{offer.name.clone()}</strong><p>{format!("{} videos proposed · Requested: {}",offer.clip_count,requirements.volume)}</p><p>{offer.terms.clone()}</p><small>"Seller-provided metadata only. Video access is separate."</small></div>})}
                        {r.offer.is_none().then(||view!{<p>{if buyer {"Waiting for the supplier to propose footage against your requirements."} else {"Assess the requirements, then select footage and explain its fit and usage terms."}}</p>})}
                        {(!buyer && !prepared).then(||view!{<form class="request-form" method="post" action=format!("/requests/{}/change?org={org}",r.id)><input type="hidden" name="revision" value=r.revision/><input type="hidden" name="action" value="offer"/>
                            <div class="request-field"><label for="source_id">"Your source dataset"</label><select id="source_id" name="source_id" required><option value="">"Select footage to propose"</option>{sources.into_iter().map(|d|view!{<option value=d.id>{format!("{} · {} videos",d.name,d.clips.len())}</option>}).collect_view()}</select></div>
                            <div class="request-field"><label for="terms">"Scope and usage terms"</label><textarea id="terms" name="terms" rows="3" maxlength="2000" required placeholder="Explain fit, shortfalls, permitted use and any agreement reference."></textarea></div><p class="muted">"Proposing footage shares its name, count and these terms with the buyer. A replacement proposal requires fresh agreement."</p><button class="button">"Propose footage"</button></form>})}
                    </section><section class="request-card"><div class="eyebrow">"03 / AGREED SCOPE"</div><h2>"Handoff to Safe sharing"</h2><p>"Agreement records the selected footage and proposed use. It does not grant video access or authorize export."</p>
                        {(buyer && r.offer.is_some() && !r.agreed && !prepared).then(||command(org,&r,"agree","Agree to proposed scope"))}
                        {(!buyer && r.agreed && !prepared).then(||command(org,&r,"prepare","Prepare for sharing"))}
                        {(!buyer && !r.agreed).then(||view!{<p class="muted">"Buyer agreement is required before preparing a package."</p>})}
                        {(buyer && r.agreed && !prepared).then(||view!{<p class="muted">"Scope agreed. The seller can now prepare your sharing package."</p>})}
                        {r.package_id.map(|id|if package_visible {view!{<a class="button primary" href=format!("/datasets/{id}?org={org}")>"Open sharing package · v1"</a>}.into_any()}else{view!{<p class="muted">"Package prepared · Waiting for the seller to finish review and grant access. Agreement alone does not unlock it."</p>}.into_any()})}
                    </section></div>
                </div>
            </section>
        },
    )
}
