# An agent prompt lives with the project that owns its job, and the setup names no machine

The prompts for the passes that run on Tandem — `reviewer`, `retro`, `consolidator` — lived outside this repo: `reviewer` and `retro` as files in the `fleet` repo, `consolidator` as a real file in `~/.config/opencode/agents/`, held by nothing but the filesystem. All three are Tandem's own passes. They read the board through `tandem-cli`, they file into the bikeshed, and they enforce the ticket model's gates. None of them is a model switch.

They were also, in their own text, bound to one machine: the consolidator read `~/projects/tandem/skills/…`, the retro read `~/projects/matt-pocock-skills/…`, and the host was wired by hand — a symlink here, a command file there. A clone of the repo on a second Mac could not reproduce the workflow, because the workflow was not in the repo.

This ADR records where the agent face lives, that it is portable, and that the passes are skills rather than host commands.

## Decision

- **Tandem's agent prompts live in this repo, in `agents/`, one markdown file per agent.** `agents/reviewer.md`, `agents/retro.md` and `agents/consolidator.md` are tracked here; the host finds them through symlinks into `~/.config/opencode/agents/`.
- **A prompt's home is the project that owns its job.** A prompt that reads Tandem's board, files into Tandem's bikeshed and enforces Tandem's gates belongs to Tandem, whichever repo it was written in. The fleet repo keeps the pair that is not Tandem's — `black` and `blue`, one prompt and two models — because they are a comparison arm, not a pass.
- **Nothing in this repo names a path on one machine.** A prompt points at a skill by its installed location (`~/.config/opencode/skills/…`), never at a checkout on somebody's disk. The install script resolves the checkout from its own location, so the same clone works wherever it is.
- **The setup is a skill, not a script.** `skills/tandem-setup/` walks an agent through wiring the host: it finds the checkout, links the skills, the agent prompts and this repo's `AGENTS.md` into the host's config directory, checks that `tandem-cli` is on PATH, and reports what it skipped. The host discovers all three by convention (`~/.config/opencode/{skills,agents}/` and `~/.config/opencode/AGENTS.md`), so no host config edit is needed — and an entry in `opencode.json` pointing at the same files would load them twice. A skill rather than a shell script, because the setup is a thing an agent does on request, it has to work for more than one host, and it belongs with the rest of the protocol.
- **The passes are skills; the repo ships no host command files.** The consolidation pass becomes `skills/tandem-consolidate/`, which dispatches the `consolidator` agent; a host may still bind its own command. The other passes are already skills (`tandem-board`, `tandem-handoff`, `tandem-memory`) and are model-invocable, so every pass is reachable by asking. A command file was a thin wrapper over a skill, held in a host directory that no repo tracked.
- **The workflow's third-party skills are vendored here.** `skills/retro/` and `skills/writing-for-agents/` come from [mattpocock/skills](https://github.com/mattpocock/skills) (MIT, attributed in `NOTICE.txt`), so a clone of this repo is the whole workflow.
- **A prompt that states a contract states the current one.** `retro.md` claimed `todo add` took "a title and nothing else", and that its `Filed by retro` line stood in for the `origin` field "until 1DA9C0BD lands". Both were true when written and false now: `todo add` takes `--status`, `--assignee`, `--lane` and `--origin` ([ADR 0020](0020-origin-field.md)), and that field is installed. The two lines were corrected in the move, so the prompt this repo owns does not contradict the CLI this repo ships.

## Why

- **An unversioned prompt is an untracked change.** `consolidator` — the sole writer of the curated notes — lived in one real file in a host directory. No git held it: no review, no history, no way to see when it changed or why. A protocol with exactly one writer needs its writer's prompt to be reviewable like everything else.
- **A prompt that names a CLI drifts from it.** `retro.md` described a `todo add` that had not taken flags for a while, and deferred to a ticket id for a field that has since shipped. The drift was invisible because the file sat in another repo, next to no contract to check it against. Here the CLI's `Usage.swift` and the prompt sit side by side, and `skills/tandem/SKILL.md` is the third copy of the same contract.
- **A machine-specific setup is not a setup.** A path like `~/projects/tandem/skills/…` works on the machine that wrote it and nowhere else; so does a hand-made symlink and a hand-written command file. The workflow was the human's, not the repo's, and it could not be handed to a second Mac.
- **The fleet's own boundary already said this.** Its README said the consolidator was "deliberately not here: it is Tandem-specific", and its `WORKFLOW.md` called the consolidator "out of this repo by decision" — the reasoning was right and the arrangement was not: Tandem-specific meant *nobody's*, in a host directory with no owner.
- **A command file duplicates a skill.** `/board` was "load the `tandem-board` skill and follow it for one pass"; `/handoff` was "load the `tandem-handoff` skill". The skill is the procedure; the command is a keystroke. The one that earned a skill was `/consolidate`: it force-bound a host file to an agent, so the protocol's front door lived outside the protocol.
- **`black` and `blue` are a different thing.** One prompt, two models, in one file each so an edit lands identically in both and the pair isolates the model. Nothing about them is Tandem's — they are builders, and they build whatever project they are pointed at.

## Decided against

- **Moving `black` and `blue` as well** — the human excluded them, and the comparison arm is the one thing in the fleet that is not a pass.
- **Copying the prompts here and leaving the fleet's copies** — two copies of one prompt is two truths; the fleet's become pointers.
- **Keeping the command files in the repo** — they were wrappers over skills, and a host's own binding is the host's business.
- **A `prompts/` directory, a subagent registry, or a prompt pack** — three files with frontmatter are already what the host reads.
- **Leaving `retro.md`'s stale lines as they were** — a move is the one moment a prompt's contract is read end to end, and the file would have landed contradicting the CLI it drives.

## Consequences

- **The fleet repo loses two files and keeps its reason.** Its `README.md` and `WORKFLOW.md` name this repo as the home of `reviewer`, `retro` and `consolidator`; the fleet's `main` still ships `black`, `blue` and `terse`.
- **The host still finds the same agents.** `~/.config/opencode/agents/{reviewer,retro,consolidator}.md` become symlinks into this repo's `agents/`, so `opencode api agent.list` is unchanged — same names, same models, same colours.
- **The working tree is the deployment.** A prompt's live copy is the file in the checkout the symlink points at, so this repo's checkout has to sit on a branch that carries `agents/` and `skills/` — the same trap `terse`'s install script reports as `dangling`.
- **A second Mac gets the same workflow** with a clone, the app, and one ask: "set up Tandem". The `tandem-setup` skill does the wiring. The `tandem-cli` the prompts call is the app's own binary, so the app has to be on `PATH` — the tap's cask links it.
- **The store is untouched**: no field, no enum, no migration, no release. Everything this repo reads and writes is unchanged.
