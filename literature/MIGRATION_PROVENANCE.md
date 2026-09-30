# Migration provenance

The public repository was renamed from `adriendellagaspera/range-based-set-reconciliation` to `adriendellagaspera/set-reconciliation`; GitHub preserves redirects and the stable crate release lineage.

The private `adriendellagaspera/rbsr-research` repository is retained as the provenance archive. Its audited migration source was:

- main: `f594bbad95314950c686b26f7cb32f789f1cab49`
- bibliography PR #96 head: `eb84a667f804e56f04d350311ce314a45895bc45`
- benchmark-boundary PR #91 head: `ae9660333cd3f20b4df3e9d9330b5443e778b862`
- selective-reliability PR #87 head: `5877ef5091cf8e4908867bb06f5e2f6015d22dec`
- earlier selective-reliability PR #81 head: `1840d3d01b36e6b7e2ba6e91337f694e1bd29892`

The source main contains 274 commits. The GitHub integration used for this migration cannot make private Git objects addressable in the public repository (attempting to reference the private main SHA from the target returned “Object does not exist”), and it exposes no repository-admin endpoint for a server-side history transfer. Therefore the private commit graph is deliberately not republished by synthesizing rewritten commits. Published files carry exact source provenance, while the original private repository retains the authoritative historical graph.

Before publishing migrated content, the current source tree and all four open PR payloads were checked for common credential/key signatures; searches of issue/PR text found no private-user-image attachments. GitHub's native Secret Scanning alert API is not exposed by the connected integration, so the full private history was not declared clean or published.


## Public migration result

The unification landed in [set-reconciliation#18](https://github.com/adriendellagaspera/set-reconciliation/pull/18) with merge commit `c2de32cba9ee05e4421d9ffdf428e0459459a702`. The stable package versions were unchanged (`rsos 0.5.2`, `rbsr 0.2.3`), and the existing release lineage remains intact.

The useful open research tracker was recreated in the public repository with source provenance:

| Source | Public |
|---|---|
| #5, #6, #10, #11, #13 | #59, #58, #57, #56, #55 |
| #20, #21, #23–#31 | #54, #53, #52–#44 |
| #36, #37, #39–#41 | #43, #42, #41–#39 |
| #48, #51–#58 | #38, #37–#30 |
| #61–#63, #68–#71 | #29–#23 |
| #92–#95 | #22–#19 |

Issue comments were copied with author/date/source provenance. Bare source-tracker cross-references in migrated issue bodies were rewritten to the corresponding public issue where one exists; non-migrated historical references are kept only where they are necessary provenance.

Open source pull requests were resolved as follows:

| Source PR | Result |
|---|---|
| #96 bibliography refresh | reapplied in #18 as `literature/SURVEY.md`; source PR closed |
| #91 benchmark ownership | reapplied in #18; source PR closed |
| #87 minimal selective-reliability wire | recreated as public draft #60 |
| #81 earlier reliability wire identity | recreated as public draft #61 |

Both public reliability drafts are retained independently because their source branches diverged and carried distinct work. They remain drafts by design; their permanent experimental CI is green after path/module adaptation.

The private source repository now carries an archive banner and no open issues or pull requests. Its repository-level GitHub `archived` flag could not be set through the available integration because no repository-admin mutation endpoint is exposed; the repository therefore remains private and writable at the platform level even though it is operationally treated as provenance-only.
