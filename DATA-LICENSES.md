# Data licences: The Stacks web edition

The code is MIT. The texts are not ours: each source keeps its own licence, listed below
with the exact number of records this edition carries from it. This page is generated from
the data file itself (`tools/export_web_db.py licenses`). File built 2026-11-02.

## Included

- **ORAEC (Simon D. Schweitzer, BBAW)**: CC BY-SA 4.0. 13,026 texts, 101,796 sentences. <https://github.com/oraec/corpus_raw_data>
- **AED-TEI (Simon D. Schweitzer, BBAW)**: CC BY-SA 4.0. 13,748 texts, 98,265 sentences. <https://github.com/simondschweitzer/aed-tei>
- **Ramsès Late Egyptian Corpus (Université de Liège)**: CC BY-NC-SA 4.0. **Non-commercial use only.** 3 texts, 71,263 sentences. <https://ramses.ulg.ac.be>
- **Coptic Scriptorium**: CC BY 4.0. 1,467 texts, 51,474 sentences. <https://copticscriptorium.org>
- **Thesaurus Linguae Aegyptiae (Hugging Face)**: CC BY-SA 4.0. 29,762 texts, 29,762 sentences. <https://huggingface.co/thesaurus-linguae-aegyptiae>
- **Aegyptus manual entries**: public domain. 5 texts, 254 sentences.
- **papyri.info (DDbDP / DCLP / IDP)**: CC BY 3.0. 19,207 texts, 0 sentences. <https://papyri.info>
- **The Metropolitan Museum of Art Open Access**: CC0. 248 texts, 0 sentences. <https://www.metmuseum.org/about-the-met/policies-and-documents/open-access>
- **Nederhof HieroJax (names only, no text)**: names only. 120 texts, 0 sentences. <https://nederhof.github.io/hierojax/>

3 texts hold RAMSES sentences. Every RAMSES sentence carries the non-commercial
flag in the file and on the page. RAMSES arrives as its dataset splits (train/test/val),
not as separate texts, so each split is one very long volume.

## Left out

- `nederhof_philologeg`: redistribution needs the author's permission.
- `aes`: unordered near-duplicate of ORAEC rows (97,975 of 101,796 identical).
- `ad_hoc`: test fixture.
- `test`: test fixture.
- `test_suite`: test fixture.

Concordance links (Trismegistos and others) are not in this edition.
