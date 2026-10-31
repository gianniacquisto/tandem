//! The TypeScript declarations, generated from the wire's own types.
//!
//! `docs/port.md` (Stack): the message types are `ts-rs`'s, generated from the
//! Rust structs, "so a shape cannot drift without a compile error on both
//! sides". This module is the list of what is exported and the one rendering
//! both the exporter and the staleness test use: `files()` is the truth, the
//! committed files under `app/renderer/src/wire/` are its copy, and
//! `tests/wire_types.rs` fails when the copy stops matching.
//!
//! Regenerate with `cargo run -p tandem-core --example export-wire`.

use ts_rs::{Config, TS};

use super::rpc::{
    ArchivedReply, CommentAddParams, CommentListReply, CommentReply, DoctorReply, EmptyParams,
    Event, HostRoot, LaneAddParams, LaneDeleteParams, LaneDeletionReply, LaneListReply,
    LaneRenameParams, MarkdownBlocksParams, MarkdownReply, Method, NoteBacklinksReply,
    NoteHealthReply, NoteMoveParams, NoteMoveReply, NotePathParams, NoteReadReply,
    NoteRecoveriesReply, NoteTreeParams, NoteTreeReply, NoteWriteParams, NoteWriteReply, Outcome,
    Reply, Request, Response, SetupInstallParams, SetupReply, TicketAddFromNoteParams,
    TicketAddParams, TicketAnswerParams, TicketArchiveParams, TicketBlockParams, TicketIdParams,
    TicketListParams, TicketListReply, TicketMoveParams, TicketReply, TicketSetParams,
    TicketUnblockParams, UsageReportReply, WireError,
};
use super::views::{
    BacklinkView, CommentView, MarkdownBlockView, MarkdownInlineView, MovedNoteView,
    NoteContentView, NoteMoveView, NoteNodeView, NoteWriteView, RecoveryView, TicketBlockView,
    TicketDetail, TicketSummary, UsageReportView, UsageRowView, UsageSessionView, UsageTokensView,
};
use super::{DoctorCheck, DoctorReport};
use crate::bikeshed::{BikeshedHealth, BikeshedHealthCounts, BrokenLink};
use crate::setup;

/// Every generated file, by name, in a stable order.
///
/// `i64` is a JavaScript `number`: the wire's revisions and counts are small
/// enough to be exact in a double, and a `bigint` would make every consumer
/// unwrap it.
pub fn files() -> Vec<(&'static str, String)> {
    let cfg = Config::new().with_large_int("number");
    let render = |name: &'static str, body: Result<String, ts_rs::ExportError>| {
        (
            name,
            body.unwrap_or_else(|error| panic!("{name} is exportable: {error}")),
        )
    };
    vec![
        render("Request.ts", Request::export_to_string(&cfg)),
        render("Method.ts", Method::export_to_string(&cfg)),
        render("Response.ts", Response::export_to_string(&cfg)),
        render("Outcome.ts", Outcome::export_to_string(&cfg)),
        render("Reply.ts", Reply::export_to_string(&cfg)),
        render("Event.ts", Event::export_to_string(&cfg)),
        render("WireError.ts", WireError::export_to_string(&cfg)),
        render("EmptyParams.ts", EmptyParams::export_to_string(&cfg)),
        render(
            "TicketListParams.ts",
            TicketListParams::export_to_string(&cfg),
        ),
        render("TicketIdParams.ts", TicketIdParams::export_to_string(&cfg)),
        render(
            "TicketAddParams.ts",
            TicketAddParams::export_to_string(&cfg),
        ),
        render(
            "TicketAddFromNoteParams.ts",
            TicketAddFromNoteParams::export_to_string(&cfg),
        ),
        render(
            "TicketSetParams.ts",
            TicketSetParams::export_to_string(&cfg),
        ),
        render(
            "TicketMoveParams.ts",
            TicketMoveParams::export_to_string(&cfg),
        ),
        render(
            "TicketBlockParams.ts",
            TicketBlockParams::export_to_string(&cfg),
        ),
        render(
            "TicketUnblockParams.ts",
            TicketUnblockParams::export_to_string(&cfg),
        ),
        render(
            "TicketAnswerParams.ts",
            TicketAnswerParams::export_to_string(&cfg),
        ),
        render(
            "TicketArchiveParams.ts",
            TicketArchiveParams::export_to_string(&cfg),
        ),
        render(
            "CommentAddParams.ts",
            CommentAddParams::export_to_string(&cfg),
        ),
        render("LaneAddParams.ts", LaneAddParams::export_to_string(&cfg)),
        render(
            "LaneRenameParams.ts",
            LaneRenameParams::export_to_string(&cfg),
        ),
        render(
            "LaneDeleteParams.ts",
            LaneDeleteParams::export_to_string(&cfg),
        ),
        render(
            "TicketListReply.ts",
            TicketListReply::export_to_string(&cfg),
        ),
        render("TicketReply.ts", TicketReply::export_to_string(&cfg)),
        render("CommentReply.ts", CommentReply::export_to_string(&cfg)),
        render(
            "CommentListReply.ts",
            CommentListReply::export_to_string(&cfg),
        ),
        render("LaneListReply.ts", LaneListReply::export_to_string(&cfg)),
        render(
            "LaneDeletionReply.ts",
            LaneDeletionReply::export_to_string(&cfg),
        ),
        render("ArchivedReply.ts", ArchivedReply::export_to_string(&cfg)),
        render("SetupReply.ts", SetupReply::export_to_string(&cfg)),
        render("HostRoot.ts", HostRoot::export_to_string(&cfg)),
        render("DoctorReply.ts", DoctorReply::export_to_string(&cfg)),
        render("DoctorReport.ts", DoctorReport::export_to_string(&cfg)),
        render("DoctorCheck.ts", DoctorCheck::export_to_string(&cfg)),
        render(
            "SetupInstallParams.ts",
            SetupInstallParams::export_to_string(&cfg),
        ),
        render("Report.ts", setup::Report::export_to_string(&cfg)),
        render("PathAction.ts", setup::PathAction::export_to_string(&cfg)),
        render("State.ts", setup::State::export_to_string(&cfg)),
        render("HostState.ts", setup::HostState::export_to_string(&cfg)),
        render("Warning.ts", setup::Warning::export_to_string(&cfg)),
        render("Code.ts", setup::Code::export_to_string(&cfg)),
        render("Severity.ts", setup::Severity::export_to_string(&cfg)),
        render("TicketSummary.ts", TicketSummary::export_to_string(&cfg)),
        render("TicketDetail.ts", TicketDetail::export_to_string(&cfg)),
        render(
            "TicketBlockView.ts",
            TicketBlockView::export_to_string(&cfg),
        ),
        render("CommentView.ts", CommentView::export_to_string(&cfg)),
        render(
            "UsageReportReply.ts",
            UsageReportReply::export_to_string(&cfg),
        ),
        render(
            "UsageReportView.ts",
            UsageReportView::export_to_string(&cfg),
        ),
        render("UsageRowView.ts", UsageRowView::export_to_string(&cfg)),
        render(
            "UsageSessionView.ts",
            UsageSessionView::export_to_string(&cfg),
        ),
        render(
            "UsageTokensView.ts",
            UsageTokensView::export_to_string(&cfg),
        ),
        render("NoteTreeParams.ts", NoteTreeParams::export_to_string(&cfg)),
        render("NotePathParams.ts", NotePathParams::export_to_string(&cfg)),
        render(
            "NoteWriteParams.ts",
            NoteWriteParams::export_to_string(&cfg),
        ),
        render("NoteMoveParams.ts", NoteMoveParams::export_to_string(&cfg)),
        render("NoteTreeReply.ts", NoteTreeReply::export_to_string(&cfg)),
        render("NoteReadReply.ts", NoteReadReply::export_to_string(&cfg)),
        render("NoteWriteReply.ts", NoteWriteReply::export_to_string(&cfg)),
        render("NoteMoveReply.ts", NoteMoveReply::export_to_string(&cfg)),
        render(
            "NoteHealthReply.ts",
            NoteHealthReply::export_to_string(&cfg),
        ),
        render(
            "NoteBacklinksReply.ts",
            NoteBacklinksReply::export_to_string(&cfg),
        ),
        render(
            "NoteRecoveriesReply.ts",
            NoteRecoveriesReply::export_to_string(&cfg),
        ),
        render("NoteNodeView.ts", NoteNodeView::export_to_string(&cfg)),
        render(
            "NoteContentView.ts",
            NoteContentView::export_to_string(&cfg),
        ),
        render("NoteWriteView.ts", NoteWriteView::export_to_string(&cfg)),
        render("NoteMoveView.ts", NoteMoveView::export_to_string(&cfg)),
        render("MovedNoteView.ts", MovedNoteView::export_to_string(&cfg)),
        render("BacklinkView.ts", BacklinkView::export_to_string(&cfg)),
        render("RecoveryView.ts", RecoveryView::export_to_string(&cfg)),
        render("BikeshedHealth.ts", BikeshedHealth::export_to_string(&cfg)),
        render("BrokenLink.ts", BrokenLink::export_to_string(&cfg)),
        render(
            "BikeshedHealthCounts.ts",
            BikeshedHealthCounts::export_to_string(&cfg),
        ),
        render(
            "MarkdownBlocksParams.ts",
            MarkdownBlocksParams::export_to_string(&cfg),
        ),
        render("MarkdownReply.ts", MarkdownReply::export_to_string(&cfg)),
        render(
            "MarkdownBlockView.ts",
            MarkdownBlockView::export_to_string(&cfg),
        ),
        render(
            "MarkdownInlineView.ts",
            MarkdownInlineView::export_to_string(&cfg),
        ),
    ]
}
