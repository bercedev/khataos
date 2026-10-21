# Search and launch checklist

**For maintainers only.** These steps need repository admin rights or a personal account, so they can't be done in a pull request. This page is left out of the docs site (see `exclude_docs` in `mkdocs.yml`).

## On GitHub

- [ ] **Upload the social preview.** Settings → General → Social preview: upload `branding/concepts/1-spine/social-preview.png` (1280 × 640). Links shared on social media and in chat then show the KhataOS card.
- [ ] **Turn on the docs site.** Follow "The docs site" in [`docs/README.md`](README.md): Pages source set to GitHub Actions, and the `DOCS_PAGES` variable set to `true`. The site is then at https://khataos.github.io/khataos/, with a sitemap.
- [ ] **Set the repository homepage** (the cog next to About) to https://khataos.github.io/khataos/.
- [ ] **Set the About description and topics.** For example: "Free, open source accounting software that works offline: GST, VAT, ZATCA e-invoicing, UK MTD. A Tally alternative." Topics: `accounting`, `accounting-software`, `gst`, `vat`, `zatca`, `e-invoicing`, `making-tax-digital`, `tally`, `tauri`, `rust`, `offline-first`, `bookkeeping`.
- [ ] **Publish a GitHub release** as soon as the first builds exist. Releases show on the repository page, and they rank in search.
- [ ] **Submit the sitemap** (https://khataos.github.io/khataos/sitemap.xml) in Google Search Console and Bing Webmaster Tools once the site is live.

## Lists and directories

Read each list's rules before submitting. Some want a project that has had releases for a few months.

- [ ] awesome-selfhosted, the money and accounting section: https://github.com/awesome-selfhosted/awesome-selfhosted
- [ ] awesome-rust, applications: https://github.com/rust-unofficial/awesome-rust
- [ ] AlternativeTo, listed as an alternative to Tally: https://alternativeto.net
- [ ] Product Hunt: https://www.producthunt.com

## Launch

- [ ] **Write a short launch post**: what KhataOS is, who it's for, the alpha status, and how to try it and report problems. Post it in GitHub Discussions and link it from the release.
- [ ] **Later: a project website and domain.** Point a custom domain at the docs site, or build a small landing page, once there is a stable release.

When writing about KhataOS, use "Tally" and "TallyPrime" only to describe compatibility ("an alternative to TallyPrime", "imports from Tally"), and never suggest any link with Tally Solutions.
