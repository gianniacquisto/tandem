# 04 - Model-invoked skill mechanics: discovery, description wording, global install

Type: research
Status: claimed

## Question

The destination skill (`skills/tandem-memory/` in this repo) must be model-invoked: the agent fires it on its own at the moments that matter (orienting at session start; recording highlights as work happens), with no human typing its name. Research against primary sources (OpenCode and Claude Code documentation and source):

1. **Discovery**: where each harness looks for skills — global and per-repo directories — and the file shape each requires (SKILL.md location, frontmatter fields); which frontmatter makes a skill model-invoked vs user-invoked in each harness.
2. **Invocation semantics**: when the harness loads a model-invoked skill's description into context, and how firing is decided; what description wording reliably fires on the two trigger branches (session-start orientation; recording work highlights) without spurious firing mid-task.
3. **Install/link**: how a skill stored at `skills/tandem-memory/` in this repo is best exposed globally for both harnesses — symlink vs copy, exact target directories, and what happens when the repo is updated.

Deliverable: a per-harness recommendation (discovery paths, required frontmatter, description-branch wording, install mechanism) with links to key sources. Capture the findings as a report on a throwaway `research/tandem-memory-model-invocation` branch (report file under `.scratch/tandem-memory/research/`), pushed but never merged, no PR.
