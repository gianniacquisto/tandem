//! The People screen (CUT-42, ADR 0052): an admin adds people, issues their
//! sign-in keys, changes roles, removes people, revokes a member's personal token
//! and reads the access record.
//!
//! The pages are a face over the store functions (`members.rs`, `access_record.rs`)
//! and hold no SQL: serve is a second caller of the one writer. The guard refuses
//! a member and a personal token on every route here by the permission table
//! (`manage-people`), so a `403` is never drawn from a half-built list. The
//! handlers check team mode (else `404`) and the principal's shape, then act.
//!
//! **A sign-in key is shown once.** Adding a person and reissuing a key answer the
//! page that shows the key, `no-store`, the way a personal token is shown (CUT-41).
//! The list and the detail never carry one, and nothing logs one. The add form's
//! idempotence key stops a double submit from adding two people.
//!
//! The owner (the `TANDEM_TOKEN` holder, signed in) is the one bootstrap: outside
//! team mode the owner's token-mode session may reach this screen to add the first
//! admin, because team mode starts with the first member.

// Every fallible helper here answers a response, the shape a refusal takes in
// this serve; a box would only move it a layer out (see the gate).
#![allow(clippy::result_large_err)]

use super::guard::{self, Principal, refuse_access};
use super::tokens::{TokenKeys, date, fresh_key, plain_failure, see_other, shell, well_formed_key};
use super::*;
use crate::store::Member;
use crate::store::{
    AccessActor, AccessRow, MemberError, MemberState, Role, TokenInfo, member_name_rule, unix_now,
};

const COULD_NOT_SAVE: &str = "Could not save. Nothing changed. Try again.";
const NAME_RULE: &str = "Use 1 to 64 lowercase letters, digits or hyphens.";
const NAME_RESERVED: &str = "That name is reserved. Choose another.";
const NAME_TAKEN: &str = "That name is already taken.";
const ROLE_RULE: &str = "That role is not one of admin or member.";
const FIRST_ADMIN: &str = "The first person must be an admin. Add an admin first.";
const LAST_ADMIN: &str = "This is the only admin. Make someone else an admin first.";
const NOT_FOUND_PERSON: &str = "That person was not found.";
const NOT_FOUND_TOKEN: &str = "That token was not found.";
const OWNER_LEAD: &str =
    "You are signed in as the workspace owner. The owner is not a member and takes no seat.";

/// How many access-record rows a page holds.
const RECORD_PAGE: usize = 50;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Who asks: the owner, or an admin's browser session.
enum Who {
    Owner,
    Admin(Member),
}

impl Who {
    fn name(&self) -> Option<&str> {
        match self {
            Self::Owner => None,
            Self::Admin(member) => Some(member.name.as_str()),
        }
    }
}

/// The first checks every handler makes: team mode, or the owner's session in
/// token mode (the bootstrap), else `404`; then the principal. The guard has
/// already refused a member and a token on this screen's routes.
// The rejection is the response the request gets, the shape every extractor
// answers a refusal in; a box would only move it a layer out.
#[allow(clippy::result_large_err)]
fn gate(context: &Context) -> Result<Who, Response> {
    let team = match context.store.team_mode() {
        Ok(team) => team,
        Err(error) => {
            tracing::error!(error = ?error, "the People screen could not read the workspace mode");
            return Err(plain_failure());
        }
    };
    let bootstrap = matches!(context.principal, Principal::Owner) && context.access.is_token();
    if !team && !bootstrap {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    match &context.principal {
        Principal::Owner => Ok(Who::Owner),
        Principal::MemberSession(member) if member.role == Role::Admin => {
            Ok(Who::Admin(member.clone()))
        }
        Principal::MemberSession(_) | Principal::MemberToken(_) | Principal::Operator => {
            Err(refuse_access("/people", "only an admin manages people"))
        }
    }
}

/// `16 Nov 2026 21:40`, UTC: the time an access-record row reads with.
fn stamp(seconds: i64) -> String {
    let (year, month, day) = sheet::civil_from_days(seconds.div_euclid(86_400));
    let minutes = seconds.rem_euclid(86_400) / 60;
    format!(
        "{day} {} {year} {:02}:{:02}",
        MONTHS[(month - 1) as usize],
        minutes / 60,
        minutes % 60
    )
}

/// One access-record row as a plain sentence. A row holds no token, key or digest,
/// so none reaches the sentence.
fn sentence(row: &AccessRow) -> String {
    let who = row.actor.as_str();
    let target = row.target.as_deref().unwrap_or("");
    let detail = row.detail.as_deref().unwrap_or("");
    let text = match row.event.as_str() {
        "member-added" => format!("{who} added {target} as {detail}"),
        "member-removed" => format!("{who} removed {target}"),
        "member-role-changed" => format!("{who} changed {target} to {detail}"),
        "token-created" if who == target => format!("{who} created a personal token"),
        "token-created" => format!("{who} created a personal token for {target}"),
        "token-revoked" if who == target => format!("{who} revoked a personal token"),
        "token-revoked" => format!("{who} revoked a personal token of {target}"),
        "signin-key-issued" => format!("{who} issued a sign-in key for {target}"),
        "export" => format!("{who} exported the workspace"),
        other => format!("{who} {other}"),
    };
    format!("{text}, {}", stamp(row.at))
}

/// A notice a list page redraws with, from a fixed vocabulary: the query never
/// reaches the page as text.
fn list_notice(code: &str) -> Option<String> {
    match code {
        "role-changed" => Some("The role was changed.".to_string()),
        "role-unchanged" => Some("Nothing changed.".to_string()),
        "duplicate" => Some("That person was already added. Nothing changed.".to_string()),
        _ => None,
    }
}

/// The detail page's notices, from the same fixed vocabulary.
fn detail_notice(code: &str) -> Option<&'static str> {
    match code {
        "token-revoked" => Some("Token revoked."),
        "already-revoked" => Some("Already revoked."),
        _ => None,
    }
}

/// The one sentence a name rule is answered with, tied to the name field.
fn name_sentence(rule: &str) -> &'static str {
    if rule == "is reserved" {
        NAME_RESERVED
    } else {
        NAME_RULE
    }
}

#[derive(Template)]
#[template(path = "people.html")]
struct PeopleView {
    owner_lead: Option<&'static str>,
    notice: Option<String>,
    error: Option<String>,
    rows: Vec<PersonRow>,
    name: String,
    role: String,
    key: String,
    name_error: Option<&'static str>,
    role_error: Option<&'static str>,
    record: Vec<String>,
    older: Option<String>,
}

struct PersonRow {
    name: String,
    role: &'static str,
    role_label: &'static str,
    you: bool,
    last_seen: String,
    tokens: usize,
    last_admin: bool,
}

#[derive(Template)]
#[template(path = "person.html")]
struct PersonView {
    notice: Option<&'static str>,
    name: String,
    role_label: &'static str,
    last_seen: String,
    tokens: Vec<TokenRow>,
}

struct TokenRow {
    id: i64,
    label: String,
    created: String,
    last_used: String,
}

#[derive(Template)]
#[template(path = "people_key.html")]
struct KeyView {
    heading: String,
    name: String,
    key: String,
}

#[derive(Template)]
#[template(path = "people_confirm.html")]
struct ConfirmView {
    title: &'static str,
    lead: String,
    action: String,
    submit: &'static str,
    fields: Vec<Field>,
    back: String,
}

/// A hidden field a confirm page carries to its own act.
struct Field {
    name: &'static str,
    value: &'static str,
}

#[derive(Template)]
#[template(path = "people_message.html")]
struct MessageView {
    text: String,
}

/// The shell around one of these pages. Never cached and no change stream: the
/// page may hold a key, and a key is not redrawn under the admin's cursor.
fn page(context: &Context, body: Result<String, askama::Error>, status: StatusCode) -> Response {
    shell(context, body, status)
}

fn message(context: &Context, text: String, status: StatusCode) -> Response {
    page(context, MessageView { text }.render(), status)
}

fn not_found_person(context: &Context) -> Response {
    message(context, NOT_FOUND_PERSON.to_string(), StatusCode::NOT_FOUND)
}

fn already_removed(context: &Context, name: &str) -> Response {
    message(
        context,
        format!("{name} was already removed."),
        StatusCode::OK,
    )
}

/// A redirect that also ends the browser session it was sent from: the cookie is
/// cleared, the server-side session already gone with the member.
fn signed_out_redirect(location: &str) -> Response {
    let mut response = see_other(location);
    response
        .headers_mut()
        .append(header::SET_COOKIE, guard::session_cookie("", 0));
    response
}

/// The person a path names, or the answer the state calls for: `Err` is the
/// response to send, `Ok` the live member.
async fn live_person(context: &Context, name: &str) -> Result<Member, Response> {
    match person_state(context, name).await? {
        MemberState::Live(member) => Ok(member),
        MemberState::Removed => Err(already_removed(context, name)),
        MemberState::Unknown => Err(not_found_person(context)),
    }
}

async fn person_state(context: &Context, name: &str) -> Result<MemberState, Response> {
    let store = context.store.clone();
    let name = name.to_string();
    match tokio::task::spawn_blocking(move || store.member_state(&name)).await {
        Ok(Ok(state)) => Ok(state),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "a person could not be read");
            Err(plain_failure())
        }
        Err(_) => Err(plain_failure()),
    }
}

/// Every live member with their counts, or the failure sentence.
async fn people(context: &Context) -> Result<Vec<crate::store::PersonSummary>, Response> {
    let store = context.store.clone();
    match tokio::task::spawn_blocking(move || store.people()).await {
        Ok(Ok(people)) => Ok(people),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "the people could not be read");
            Err(plain_failure())
        }
        Err(_) => Err(plain_failure()),
    }
}

/// Whether `name` is the workspace's only admin: the one act a last-admin rule
/// refuses by every door.
async fn is_last_admin(context: &Context, name: &str) -> Result<bool, Response> {
    let people = people(context).await?;
    let admins = people
        .iter()
        .filter(|person| person.member.role == Role::Admin)
        .count();
    Ok(admins <= 1
        && people
            .iter()
            .any(|person| person.member.name == name && person.member.role == Role::Admin))
}

/// The add form's and the list's draft: what a page redraws with.
struct Draft {
    notice: Option<String>,
    error: Option<String>,
    name: String,
    role: String,
    key: String,
    name_error: Option<&'static str>,
    role_error: Option<&'static str>,
}

impl Draft {
    fn clean(notice: Option<String>) -> Self {
        Self {
            notice,
            error: None,
            name: String::new(),
            role: "member".to_string(),
            key: fresh_key(),
            name_error: None,
            role_error: None,
        }
    }

    /// The fields the admin typed, kept, with no error to show.
    fn kept(name: String, role: String, key: String) -> Self {
        Self {
            notice: None,
            error: None,
            name,
            role,
            key,
            name_error: None,
            role_error: None,
        }
    }

    fn failed(error: &str, name: String, role: String, key: String) -> Self {
        Self {
            notice: None,
            error: Some(error.to_string()),
            name,
            role,
            key,
            name_error: None,
            role_error: None,
        }
    }
}

async fn list_page(
    context: &Context,
    who: &Who,
    before: Option<i64>,
    draft: Draft,
    status: StatusCode,
) -> Response {
    let people = match people(context).await {
        Ok(people) => people,
        Err(response) => return response,
    };
    let store = context.store.clone();
    let mut record = match tokio::task::spawn_blocking(move || {
        store.access_record_page(before, RECORD_PAGE + 1)
    })
    .await
    {
        Ok(Ok(record)) => record,
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "the access record could not be read");
            return plain_failure();
        }
        Err(_) => return plain_failure(),
    };
    let older = if record.len() > RECORD_PAGE {
        record.truncate(RECORD_PAGE);
        record.last().map(|row| row.id.to_string())
    } else {
        None
    };
    let admins = people
        .iter()
        .filter(|person| person.member.role == Role::Admin)
        .count();
    let rows = people
        .iter()
        .map(|person| {
            let admin = person.member.role == Role::Admin;
            PersonRow {
                name: person.member.name.clone(),
                role: person.member.role.as_str(),
                role_label: if admin { "Admin" } else { "Member" },
                you: who.name() == Some(person.member.name.as_str()),
                last_seen: person
                    .last_signed_in
                    .map_or_else(|| "Not signed in yet".to_string(), stamp),
                tokens: person.live_tokens,
                last_admin: admin && admins == 1,
            }
        })
        .collect();
    let view = PeopleView {
        owner_lead: matches!(who, Who::Owner).then_some(OWNER_LEAD),
        notice: draft.notice,
        error: draft.error,
        rows,
        name: draft.name,
        role: draft.role,
        key: draft.key,
        name_error: draft.name_error,
        role_error: draft.role_error,
        record: record.iter().map(sentence).collect(),
        older,
    };
    page(context, view.render(), status)
}

pub(super) async fn list(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let before = field(&params, "before").parse::<i64>().ok();
    let draft = Draft::clean(list_notice(field(&params, "notice")));
    list_page(&context, &who, before, draft, StatusCode::OK).await
}

pub(super) async fn add(
    context: Context,
    Extension(keys): Extension<TokenKeys>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let Some(params) = form_params(form) else {
        tracing::warn!("a person add was refused: the form could not be read");
        let draft = Draft::failed(COULD_NOT_SAVE, String::new(), "member".into(), fresh_key());
        return list_page(&context, &who, None, draft, StatusCode::BAD_REQUEST).await;
    };
    let typed = field(&params, "name").to_string();
    let name = typed.trim().to_string();
    let role_text = match field(&params, "role").trim() {
        "" => "member".to_string(),
        other => other.to_string(),
    };
    let key = field(&params, "key").to_string();
    if !well_formed_key(&key) {
        tracing::warn!("a person add was refused: the form carried no usable key");
        let draft = Draft::failed(COULD_NOT_SAVE, typed, role_text, fresh_key());
        return list_page(&context, &who, None, draft, StatusCode::BAD_REQUEST).await;
    }
    let Some(role) = Role::parse(&role_text) else {
        let draft = Draft {
            role_error: Some(ROLE_RULE),
            ..Draft::kept(typed, role_text, key)
        };
        return list_page(
            &context,
            &who,
            None,
            draft,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    };
    if let Some(rule) = member_name_rule(&name) {
        let draft = Draft {
            name_error: Some(name_sentence(rule)),
            error: None,
            ..Draft::kept(typed, role_text, key)
        };
        return list_page(
            &context,
            &who,
            None,
            draft,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    // The first submit takes the key; a second one with it adds nobody.
    if !keys.claim("people", &key) {
        tracing::info!("a person add was a repeat of one already made");
        return see_other("/people?notice=duplicate");
    }

    let store = context.store.clone();
    let by = context.access_actor();
    let added = name.clone();
    let result =
        tokio::task::spawn_blocking(move || store.member_add_by(&added, role, unix_now(), &by))
            .await;
    let member = match result {
        Ok(Ok(member)) => member,
        Ok(Err(MemberError::Taken)) => {
            keys.release("people", &key);
            let draft = Draft {
                name_error: Some(NAME_TAKEN),
                ..Draft::kept(typed, role_text, key)
            };
            return list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Ok(Err(MemberError::Invalid(rule))) => {
            keys.release("people", &key);
            let draft = Draft {
                name_error: Some(name_sentence(rule)),
                ..Draft::kept(typed, role_text, key)
            };
            return list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Ok(Err(MemberError::NoAdmin)) => {
            keys.release("people", &key);
            let draft = Draft::failed(FIRST_ADMIN, typed, role_text, key);
            return list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Ok(Err(error)) => {
            keys.release("people", &key);
            tracing::error!(error = ?error, "a person could not be added");
            let draft = Draft::failed(COULD_NOT_SAVE, typed, role_text, key);
            return list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await;
        }
        Err(_) => {
            keys.release("people", &key);
            let draft = Draft::failed(COULD_NOT_SAVE, typed, role_text, key);
            return list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await;
        }
    };
    tracing::info!(member = %member.name, "a person was added");

    // Added and shown in one step: the key is minted for the person just added.
    let store = context.store.clone();
    let by = context.access_actor();
    let owner_name = member.name.clone();
    let issued = tokio::task::spawn_blocking(move || {
        store.member_signin_key_issue_by(&owner_name, unix_now(), &by)
    })
    .await;
    match issued {
        Ok(Ok(Some(credential))) => page(
            &context,
            KeyView {
                heading: format!("{} was added as {}.", member.name, member.role.as_str()),
                name: member.name.clone(),
                key: credential.reveal().to_string(),
            }
            .render(),
            StatusCode::OK,
        ),
        _ => {
            tracing::error!("a sign-in key could not be issued for a person just added");
            let draft = Draft {
                error: Some(format!(
                    "{} was added, but the sign-in key could not be issued. Use Reissue key.",
                    member.name
                )),
                ..Draft::clean(None)
            };
            list_page(
                &context,
                &who,
                None,
                draft,
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await
        }
    }
}

pub(super) async fn detail(
    context: Context,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    if let Err(response) = gate(&context) {
        return response;
    }
    let member = match live_person(&context, &name).await {
        Ok(member) => member,
        Err(response) => return response,
    };
    let people = match people(&context).await {
        Ok(people) => people,
        Err(response) => return response,
    };
    let Some(summary) = people.iter().find(|person| person.member.id == member.id) else {
        return already_removed(&context, &name);
    };
    let store = context.store.clone();
    let person = member.name.clone();
    let tokens: Vec<TokenInfo> =
        match tokio::task::spawn_blocking(move || store.member_tokens(&person)).await {
            Ok(Ok(tokens)) => tokens,
            _ => return plain_failure(),
        };
    let view = PersonView {
        notice: detail_notice(field(&params, "notice")),
        name: member.name.clone(),
        role_label: if member.role == Role::Admin {
            "Admin"
        } else {
            "Member"
        },
        last_seen: summary
            .last_signed_in
            .map_or_else(|| "Not signed in yet".to_string(), stamp),
        tokens: tokens
            .iter()
            .filter(|token| token.revoked_at.is_none())
            .map(|token| TokenRow {
                id: token.id,
                label: token.label.clone(),
                created: date(token.created_at),
                last_used: token
                    .last_used_at
                    .map_or_else(|| "Never used".to_string(), date),
            })
            .collect(),
    };
    page(&context, view.render(), StatusCode::OK)
}

fn confirm(
    context: &Context,
    title: &'static str,
    lead: String,
    action: String,
    submit: &'static str,
    fields: Vec<Field>,
    back: String,
) -> Response {
    page(
        context,
        ConfirmView {
            title,
            lead,
            action,
            submit,
            fields,
            back,
        }
        .render(),
        StatusCode::OK,
    )
}

pub(super) async fn key_confirm(context: Context, Path(name): Path<String>) -> Response {
    if let Err(response) = gate(&context) {
        return response;
    }
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    confirm(
        &context,
        "Reissue the sign-in key",
        format!(
            "Reissue the sign-in key for {name}? Their current key stops working at once, and the old one cannot be restored. They sign in with the new one."
        ),
        format!("/people/{name}/key"),
        "Reissue key",
        Vec::new(),
        format!("/people/{name}"),
    )
}

pub(super) async fn key_reissue(context: Context, Path(name): Path<String>) -> Response {
    if let Err(response) = gate(&context) {
        return response;
    }
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    let store = context.store.clone();
    let by = context.access_actor();
    let target = name.clone();
    let issued = tokio::task::spawn_blocking(move || {
        store.member_signin_key_issue_by(&target, unix_now(), &by)
    })
    .await;
    match issued {
        Ok(Ok(Some(credential))) => {
            tracing::info!(member = %name, "a sign-in key was reissued");
            page(
                &context,
                KeyView {
                    heading: format!(
                        "The sign-in key for {name} was reissued. The old key no longer works."
                    ),
                    name: name.clone(),
                    key: credential.reveal().to_string(),
                }
                .render(),
                StatusCode::OK,
            )
        }
        Ok(Ok(None)) => already_removed(&context, &name),
        _ => {
            tracing::error!("a sign-in key could not be reissued");
            plain_failure()
        }
    }
}

pub(super) async fn set_role(
    context: Context,
    Path(name): Path<String>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let current = match live_person(&context, &name).await {
        Ok(member) => member,
        Err(response) => return response,
    };
    let params = form_params(form).unwrap_or_default();
    let Some(role) = Role::parse(field(&params, "role")) else {
        let draft = Draft {
            role_error: Some(ROLE_RULE),
            ..Draft::clean(None)
        };
        return list_page(
            &context,
            &who,
            None,
            draft,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    };
    if current.role == role {
        return see_other("/people?notice=role-unchanged");
    }
    let own = who.name() == Some(name.as_str());
    let demoting_self = own && role == Role::Member;
    // The last admin's own demotion is refused before the stronger confirmation.
    if demoting_self {
        match is_last_admin(&context, &name).await {
            Ok(true) => {
                let draft = Draft {
                    error: Some(LAST_ADMIN.to_string()),
                    ..Draft::clean(None)
                };
                return list_page(&context, &who, None, draft, StatusCode::CONFLICT).await;
            }
            Ok(false) => {}
            Err(response) => return response,
        }
        if field(&params, "confirmed") != "yes" {
            return confirm(
                &context,
                "Change your own role",
                "Change your own role to member? You will be signed out at once and lose access to this screen.".to_string(),
                format!("/people/{name}/role"),
                "Change my role",
                vec![
                    Field { name: "role", value: "member" },
                    Field { name: "confirmed", value: "yes" },
                ],
                "/people".to_string(),
            );
        }
    }
    let store = context.store.clone();
    let by = context.access_actor();
    let target = name.clone();
    let changed = tokio::task::spawn_blocking(move || {
        store.member_set_role_by(&target, role, unix_now(), &by)
    })
    .await;
    match changed {
        Ok(Ok(true)) if own => signed_out_redirect("/login?notice=role-changed"),
        Ok(Ok(true)) => see_other("/people?notice=role-changed"),
        Ok(Ok(false)) => see_other("/people?notice=role-unchanged"),
        Ok(Err(MemberError::LastAdmin)) => {
            let draft = Draft {
                error: Some(LAST_ADMIN.to_string()),
                ..Draft::clean(None)
            };
            list_page(&context, &who, None, draft, StatusCode::CONFLICT).await
        }
        _ => {
            tracing::error!("a role could not be changed");
            plain_failure()
        }
    }
}

pub(super) async fn remove_confirm(context: Context, Path(name): Path<String>) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    match is_last_admin(&context, &name).await {
        Ok(true) => {
            return message(&context, LAST_ADMIN.to_string(), StatusCode::CONFLICT);
        }
        Ok(false) => {}
        Err(response) => return response,
    }
    let (lead, back) = if who.name() == Some(name.as_str()) {
        (
            "Remove yourself? You will be signed out at once and lose access to this workspace. Your past tickets and comments stay under your name. This cannot be undone.".to_string(),
            "/people".to_string(),
        )
    } else {
        (
            format!(
                "Remove {name}? They are signed out now and their personal tokens stop working. Their past tickets and comments stay under their name. This cannot be undone."
            ),
            format!("/people/{name}"),
        )
    };
    confirm(
        &context,
        "Remove person",
        lead,
        format!("/people/{name}/remove"),
        "Remove",
        Vec::new(),
        back,
    )
}

pub(super) async fn remove(context: Context, Path(name): Path<String>) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    let store = context.store.clone();
    let by = context.access_actor();
    let target = name.clone();
    let removed =
        tokio::task::spawn_blocking(move || store.member_remove_by(&target, unix_now(), &by)).await;
    match removed {
        Ok(Ok(true)) if who.name() == Some(name.as_str()) => {
            tracing::info!("an admin removed themselves");
            signed_out_redirect("/login?notice=removed")
        }
        Ok(Ok(true)) => {
            tracing::info!("a person was removed");
            let draft = Draft {
                notice: Some(format!("{name} was removed.")),
                ..Draft::clean(None)
            };
            list_page(&context, &who, None, draft, StatusCode::OK).await
        }
        Ok(Ok(false)) => already_removed(&context, &name),
        Ok(Err(MemberError::LastAdmin)) => {
            message(&context, LAST_ADMIN.to_string(), StatusCode::CONFLICT)
        }
        _ => {
            tracing::error!("a person could not be removed");
            plain_failure()
        }
    }
}

/// The person's personal token a path names, live or revoked, or `None`.
async fn token_of(context: &Context, name: &str, id: &str) -> Result<Option<TokenInfo>, Response> {
    let Ok(id) = id.parse::<i64>() else {
        return Ok(None);
    };
    let store = context.store.clone();
    let person = name.to_string();
    match tokio::task::spawn_blocking(move || store.member_tokens(&person)).await {
        Ok(Ok(tokens)) => Ok(tokens.into_iter().find(|token| token.id == id)),
        _ => Err(plain_failure()),
    }
}

pub(super) async fn token_confirm(
    context: Context,
    Path((name, id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = gate(&context) {
        return response;
    }
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    match token_of(&context, &name, &id).await {
        Err(response) => response,
        Ok(None) => message(&context, NOT_FOUND_TOKEN.to_string(), StatusCode::NOT_FOUND),
        Ok(Some(token)) if token.revoked_at.is_some() => {
            message(&context, "Already revoked.".to_string(), StatusCode::OK)
        }
        Ok(Some(token)) => confirm(
            &context,
            "Revoke token",
            format!(
                "Revoke '{}' for {name}? Tools using it stop working at once. This cannot be undone.",
                token.label
            ),
            format!("/people/{name}/tokens/{}/revoke", token.id),
            "Revoke token",
            Vec::new(),
            format!("/people/{name}"),
        ),
    }
}

pub(super) async fn token_revoke(
    context: Context,
    Path((name, id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = gate(&context) {
        return response;
    }
    if let Err(response) = live_person(&context, &name).await {
        return response;
    }
    let token = match token_of(&context, &name, &id).await {
        Err(response) => return response,
        Ok(None) => {
            return message(&context, NOT_FOUND_TOKEN.to_string(), StatusCode::NOT_FOUND);
        }
        Ok(Some(token)) => token,
    };
    if token.revoked_at.is_some() {
        return see_other(&format!("/people/{name}?notice=already-revoked"));
    }
    let store = context.store.clone();
    let by: AccessActor = context.access_actor();
    let person = name.clone();
    let revoked = tokio::task::spawn_blocking(move || {
        store.member_token_revoke_by(&person, token.id, unix_now(), &by)
    })
    .await;
    match revoked {
        Ok(Ok(true)) => {
            tracing::info!("a personal token was revoked by an admin");
            see_other(&format!("/people/{name}?notice=token-revoked"))
        }
        Ok(Ok(false)) => see_other(&format!("/people/{name}?notice=already-revoked")),
        _ => {
            tracing::error!("a personal token could not be revoked by an admin");
            plain_failure()
        }
    }
}
