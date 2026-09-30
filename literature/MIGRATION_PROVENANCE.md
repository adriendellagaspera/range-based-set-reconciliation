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
