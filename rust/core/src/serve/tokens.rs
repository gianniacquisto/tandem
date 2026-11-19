//! The personal tokens screen (CUT-41, ADR 0052): every member makes, lists and
//! revokes the personal tokens their AI coding agents use.
//!
//! The pages are a face over the store functions part 1 built and hold no SQL.
//! They answer the browser session's principal only: the guard already refuses
//! a personal token and the operator key on every page, and `manage-own-tokens`
//! (CUT-39) is the permission the route map asks. Outside team mode the whole
//! family answers `404`, as `/login` does outside token mode.
//!
//! **A token is made and shown in one step.** `POST /tokens` answers the page
//! that shows the value, `no-store`; there is no redirect, no flash and no later
//! list that carries it. The form carries an idempotence key the server claims
//! before it creates, so a double submit makes one token and shows one. A create
//! that fails releases the key, so the retry is the same form. The value never
//! enters a URL, a log line, the access record or a template other than the one
//! that shows it.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use super::guard::{self, Principal, refuse};
use super::*;
use crate::store::{
    AccessActor, LabelRule, MAX_LIVE_TOKENS, Member, TokenError, TokenInfo, token_label_rule,
    unix_now,
};

const COULD_NOT_SAVE: &str = "Could not save. Nothing changed. Try again.";
const OWNER_HAS_NONE: &str = "Personal tokens belong to members. Sign in as a member to make one.";
const NOTICE_REVOKED: &str = "Token revoked.";
const NOTICE_ALREADY_REVOKED: &str = "Already revoked.";
const NOTICE_DUPLICATE: &str =
    "That token was already made. If you did not copy it, revoke it and make a new one.";

/// How many used keys the server remembers: the oldest are forgotten first. A
/// key is random and a form is submitted once, so the window is wide.
const REMEMBERED_KEYS: usize = 4096;

/// The idempotence keys the create form has used, per member. In memory: a key
/// only has to outlive a double click, and a restart forgets it harmlessly (the
/// next page draws a fresh one).
#[derive(Clone, Default)]
pub(super) struct TokenKeys(Arc<Mutex<UsedKeys>>);

#[derive(Default)]
struct UsedKeys {
    set: HashSet<String>,
    order: VecDeque<String>,
}

impl TokenKeys {
    /// Take the key: `true` for the first caller, `false` once it is used.
    pub(super) fn claim(&self, member: &str, key: &str) -> bool {
        let scoped = format!("{member}/{key}");
        let mut used = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if !used.set.insert(scoped.clone()) {
            return false;
        }
        used.order.push_back(scoped);
        while used.order.len() > REMEMBERED_KEYS {
            if let Some(oldest) = used.order.pop_front() {
                used.set.remove(&oldest);
            }
        }
        true
    }

    /// Give a key back after a create that did not happen, so the same form
    /// can be submitted again.
    pub(super) fn release(&self, member: &str, key: &str) {
        let scoped = format!("{member}/{key}");
        let mut used = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if used.set.remove(&scoped) {
            used.order.retain(|kept| *kept != scoped);
        }
    }
}

/// A key the form carries: 32 lowercase hex characters.
pub(super) fn well_formed_key(key: &str) -> bool {
    key.len() == 32
        && key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(super) fn fresh_key() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        tracing::error!("the system random source is unavailable");
    }
    guard::hex(&bytes)
}

/// Who asks: a member (by browser session), or the owner, who has no tokens.
enum Who {
    Member(Member),
    Owner,
}

/// The first checks every handler makes: team mode (else `404`) and the
/// principal. The guard has already refused a token and the operator.
// The rejection is the response the request gets, the shape every extractor
// answers a refusal in; a box would only move it a layer out.
#[allow(clippy::result_large_err)]
fn gate(context: &Context) -> Result<Who, Response> {
    match context.store.team_mode() {
        Ok(true) => {}
        Ok(false) => return Err(StatusCode::NOT_FOUND.into_response()),
        Err(error) => {
            tracing::error!(error = ?error, "the tokens screen could not read the workspace mode");
            return Err(plain_failure());
        }
    }
    match &context.principal {
        Principal::Owner => Ok(Who::Owner),
        Principal::MemberSession(member) => Ok(Who::Member(member.clone())),
        Principal::MemberToken(_) | Principal::Operator => Err(guard::refuse_access(
            "/tokens",
            "only a browser session holds personal tokens",
        )),
    }
}

pub(super) fn plain_failure() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("{COULD_NOT_SAVE}\n"),
    )
        .into_response()
}

pub(super) fn see_other(location: &str) -> Response {
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(location).unwrap_or_else(|_| HeaderValue::from_static("/tokens")),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `YYYY-MM-DD`, UTC, from whole seconds.
pub(super) fn date(seconds: i64) -> String {
    let (year, month, day) = sheet::civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

struct TokenRow {
    id: i64,
    label: String,
    created: String,
    last_used: String,
}

impl From<&TokenInfo> for TokenRow {
    fn from(token: &TokenInfo) -> Self {
        Self {
            id: token.id,
            label: token.label.clone(),
            created: date(token.created_at),
            last_used: token
                .last_used_at
                .map_or_else(|| "Never used".to_string(), date),
        }
    }
}

#[derive(Template)]
#[template(path = "tokens.html")]
struct TokensView {
    owner: bool,
    notice: Option<&'static str>,
    tokens: Vec<TokenRow>,
    at_limit: bool,
    limit: usize,
    key: String,
    name: String,
    field_error: Option<&'static str>,
    form_error: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "token_created.html")]
struct CreatedView {
    label: String,
    token: String,
}

#[derive(Template)]
#[template(path = "token_revoke.html")]
struct RevokeView {
    id: i64,
    label: String,
}

#[derive(Template)]
#[template(path = "token_message.html")]
struct MessageView {
    text: &'static str,
}

/// The shell around one of these pages. Never cached, and no change stream: a
/// page that may hold a token is not redrawn under the member's cursor.
pub(super) fn shell(
    context: &Context,
    body: Result<String, askama::Error>,
    status: StatusCode,
) -> Response {
    let body = match body {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(error = ?error, "a tokens page could not be drawn");
            return plain_failure();
        }
    };
    let page = PageView {
        assets: assets::urls(),
        body: askama::filters::Safe(body),
        stream: false,
        sign_out: context.access.is_token(),
        export: context.may_export(),
        tokens: context.shows_tokens(),
        people: context.shows_people(),
    };
    let mut response = match page.render() {
        Ok(html) => html_response(html, status),
        Err(error) => {
            tracing::error!(error = ?error, "the shell could not be drawn");
            return plain_failure();
        }
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn message(context: &Context, text: &'static str, status: StatusCode) -> Response {
    shell(context, MessageView { text }.render(), status)
}

fn not_found(context: &Context) -> Response {
    message(context, "That token was not found.", StatusCode::NOT_FOUND)
}

/// The member's live tokens, oldest first. Revoked ones are not listed: there
/// is nothing left to do with them.
async fn live_tokens(context: &Context, member: &Member) -> Option<Vec<TokenInfo>> {
    let store = context.store.clone();
    let name = member.name.clone();
    match tokio::task::spawn_blocking(move || store.member_tokens(&name)).await {
        Ok(Ok(tokens)) => Some(tokens),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "the personal tokens could not be read");
            None
        }
        Err(_) => None,
    }
}

/// What the list and form page is drawn with besides the tokens.
struct Draft {
    notice: Option<&'static str>,
    key: String,
    name: String,
    field_error: Option<&'static str>,
    form_error: Option<&'static str>,
}

impl Draft {
    fn clean(notice: Option<&'static str>) -> Self {
        Self {
            notice,
            key: fresh_key(),
            name: String::new(),
            field_error: None,
            form_error: None,
        }
    }
}

async fn list_page(context: &Context, who: &Who, draft: Draft, status: StatusCode) -> Response {
    let (owner, tokens) = match who {
        Who::Owner => (true, Vec::new()),
        Who::Member(member) => match live_tokens(context, member).await {
            Some(tokens) => (
                false,
                tokens
                    .iter()
                    .filter(|token| token.revoked_at.is_none())
                    .map(TokenRow::from)
                    .collect(),
            ),
            None => return plain_failure(),
        },
    };
    let view = TokensView {
        owner,
        notice: draft.notice,
        at_limit: tokens.len() >= MAX_LIVE_TOKENS,
        tokens,
        limit: MAX_LIVE_TOKENS,
        key: draft.key,
        name: draft.name,
        field_error: draft.field_error,
        form_error: draft.form_error,
    };
    shell(context, view.render(), status)
}

pub(super) async fn list(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    // A fixed vocabulary: the query never reaches the page as text.
    let notice = match field(&params, "notice") {
        "revoked" => Some(NOTICE_REVOKED),
        "already-revoked" => Some(NOTICE_ALREADY_REVOKED),
        "duplicate" => Some(NOTICE_DUPLICATE),
        _ => None,
    };
    list_page(&context, &who, Draft::clean(notice), StatusCode::OK).await
}

/// The one sentence a label rule is answered with, tied to the name field.
fn label_sentence(rule: LabelRule) -> &'static str {
    match rule {
        LabelRule::Empty => "Enter a name for the token.",
        LabelRule::TooLong => "Use 60 characters or fewer.",
        LabelRule::NotPlain => "Use one line of plain text.",
    }
}

pub(super) async fn create(
    context: Context,
    Extension(keys): Extension<TokenKeys>,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let Who::Member(member) = &who else {
        return refuse(
            StatusCode::FORBIDDEN,
            OWNER_HAS_NONE,
            "the owner has no personal tokens",
        );
    };
    let failed = |form_error, name: String, key: String| Draft {
        notice: None,
        key,
        name,
        field_error: None,
        form_error: Some(form_error),
    };
    let Some(params) = form_params(form) else {
        tracing::warn!("a token create was refused: the form could not be read");
        return list_page(
            &context,
            &who,
            failed(COULD_NOT_SAVE, String::new(), fresh_key()),
            StatusCode::BAD_REQUEST,
        )
        .await;
    };
    let typed = field(&params, "name").to_string();
    let name = typed.trim().to_string();
    let key = field(&params, "key").to_string();
    if !well_formed_key(&key) {
        tracing::warn!("a token create was refused: the form carried no usable key");
        return list_page(
            &context,
            &who,
            failed(COULD_NOT_SAVE, typed, fresh_key()),
            StatusCode::BAD_REQUEST,
        )
        .await;
    }
    if let Some(rule) = token_label_rule(&name) {
        tracing::warn!("a token create was refused: the name breaks a rule");
        return list_page(
            &context,
            &who,
            Draft {
                notice: None,
                key,
                name: typed,
                field_error: Some(label_sentence(rule)),
                form_error: None,
            },
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    // The first submit takes the key; a second one with it makes nothing.
    if !keys.claim(&member.name, &key) {
        tracing::info!("a token create was a repeat of one already made");
        return see_other("/tokens?notice=duplicate");
    }

    let store = context.store.clone();
    let by = context.access_actor();
    let owner_name = member.name.clone();
    let label = name.clone();
    let issued = tokio::task::spawn_blocking(move || {
        store.member_token_issue_by(&owner_name, &label, unix_now(), &by)
    })
    .await;
    let credential = match issued {
        Ok(Ok(Some(credential))) => credential,
        Ok(Ok(None)) => {
            keys.release(&member.name, &key);
            return refuse(
                StatusCode::FORBIDDEN,
                "You are no longer a member of this workspace.",
                "the member is not live",
            );
        }
        Ok(Err(TokenError::Limit)) => {
            keys.release(&member.name, &key);
            tracing::warn!("a token create was refused: the member is at the limit");
            return list_page(
                &context,
                &who,
                Draft {
                    notice: None,
                    key,
                    name: typed,
                    field_error: None,
                    form_error: None,
                },
                StatusCode::CONFLICT,
            )
            .await;
        }
        Ok(Err(TokenError::Label(rule))) => {
            keys.release(&member.name, &key);
            return list_page(
                &context,
                &who,
                Draft {
                    notice: None,
                    key,
                    name: typed,
                    field_error: Some(label_sentence(rule)),
                    form_error: None,
                },
                StatusCode::UNPROCESSABLE_ENTITY,
            )
            .await;
        }
        Ok(Err(TokenError::Store(error))) => {
            keys.release(&member.name, &key);
            tracing::error!(error = ?error, "a personal token could not be made");
            return list_page(
                &context,
                &who,
                failed(COULD_NOT_SAVE, typed, key),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await;
        }
        Err(_) => {
            keys.release(&member.name, &key);
            return list_page(
                &context,
                &who,
                failed(COULD_NOT_SAVE, typed, key),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
            .await;
        }
    };
    tracing::info!(member = %member.name, "a personal token was made");

    // Made and shown in one step: if the page that shows it cannot be drawn,
    // the token is revoked at once rather than left unseen.
    let page = CreatedView {
        label: name,
        token: credential.reveal().to_string(),
    }
    .render();
    if page.is_err() {
        discard_newest(&context, member).await;
    }
    shell(&context, page, StatusCode::OK)
}

/// Revoke the member's newest token: the one a failed draw would leave unseen.
async fn discard_newest(context: &Context, member: &Member) {
    let store = context.store.clone();
    let by = context.access_actor();
    let name = member.name.clone();
    let _ = tokio::task::spawn_blocking(move || {
        if let Some(newest) = store.member_tokens(&name)?.last() {
            store.member_token_revoke_by(&name, newest.id, unix_now(), &by)?;
        }
        Ok::<(), crate::store::StoreError>(())
    })
    .await;
}

/// The token a path names among the member's own: `None` is "not found", and a
/// token that is someone else's is exactly that.
async fn own_token(context: &Context, member: &Member, id: &str) -> Result<Option<TokenInfo>, ()> {
    let Ok(id) = id.parse::<i64>() else {
        return Ok(None);
    };
    let tokens = live_tokens(context, member).await.ok_or(())?;
    Ok(tokens.into_iter().find(|token| token.id == id))
}

pub(super) async fn revoke_confirm(context: Context, Path(id): Path<String>) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let Who::Member(member) = &who else {
        return not_found(&context);
    };
    match own_token(&context, member, &id).await {
        Err(()) => plain_failure(),
        Ok(None) => not_found(&context),
        Ok(Some(token)) if token.revoked_at.is_some() => {
            message(&context, NOTICE_ALREADY_REVOKED, StatusCode::OK)
        }
        Ok(Some(token)) => shell(
            &context,
            RevokeView {
                id: token.id,
                label: token.label,
            }
            .render(),
            StatusCode::OK,
        ),
    }
}

pub(super) async fn revoke(context: Context, Path(id): Path<String>) -> Response {
    let who = match gate(&context) {
        Ok(who) => who,
        Err(response) => return response,
    };
    let Who::Member(member) = &who else {
        return refuse(
            StatusCode::FORBIDDEN,
            OWNER_HAS_NONE,
            "the owner has no personal tokens",
        );
    };
    let token = match own_token(&context, member, &id).await {
        Err(()) => return plain_failure(),
        Ok(None) => return not_found(&context),
        Ok(Some(token)) => token,
    };
    if token.revoked_at.is_some() {
        return see_other("/tokens?notice=already-revoked");
    }
    let store = context.store.clone();
    let by: AccessActor = context.access_actor();
    let name = member.name.clone();
    let revoked = tokio::task::spawn_blocking(move || {
        store.member_token_revoke_by(&name, token.id, unix_now(), &by)
    })
    .await;
    match revoked {
        Ok(Ok(true)) => {
            tracing::info!(member = %member.name, "a personal token was revoked");
            see_other("/tokens?notice=revoked")
        }
        // Revoked between the read and the write: the end state.
        Ok(Ok(false)) => see_other("/tokens?notice=already-revoked"),
        Ok(Err(error)) => {
            tracing::error!(error = ?error, "a personal token could not be revoked");
            message(&context, COULD_NOT_SAVE, StatusCode::INTERNAL_SERVER_ERROR)
        }
        Err(_) => message(&context, COULD_NOT_SAVE, StatusCode::INTERNAL_SERVER_ERROR),
    }
}
