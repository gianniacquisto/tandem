// Bridge — JS side (binding contract: notes/bridge-contract.md).
//
// JS → Swift via window.webkit.messageHandlers.tandem.postMessage(JSON):
//   loaded {type:"loaded"}
//   dirty {type:"dirty", text:<full raw markdown>}        (every doc change)
//   link-opened {type:"link-opened", target:<raw target or href>}
//   checkbox-toggled {type:"checkbox-toggled", line:<1-based>, checked:<bool>}
//   font-size {type:"font-size", value:<px>}              (preset, persisted)
//   send-to-todos {type:"send-to-todos", text:<the text to file>} (the
//     selection, or the caret's checkbox line; "" when there is nothing)
//
// Swift → JS via page globals (see main.js): setDocument, insertText,
// revealLine, setNoteIndex, setFontSize, focusEditor,
// requestTodoCandidate.
//
// The JS has no file or network access — everything crosses this bridge.

export function send(message) {
  const handler =
    typeof window !== "undefined" &&
    window.webkit &&
    window.webkit.messageHandlers &&
    window.webkit.messageHandlers.tandem;
  if (handler) handler.postMessage(JSON.stringify(message));
}

export const postLoaded = () => send({ type: "loaded" });
export const postDirty = (text) => send({ type: "dirty", text });
export const postLinkOpened = (target) => send({ type: "link-opened", target });
export const postCheckboxToggled = (line, checked) =>
  send({ type: "checkbox-toggled", line, checked });
export const postFontSize = (value) => send({ type: "font-size", value });
// The text a "send to todos" action would file. Swift files the ticket —
// the core owns what a ticket made of that text is.
export const postSendToTodos = (text) => send({ type: "send-to-todos", text });
