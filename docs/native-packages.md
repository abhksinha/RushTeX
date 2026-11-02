# Packages and classes the native path takes

RushTeX compiles a document natively (the GDL job, no TeX engine) only when every class,
package and command in it is ported; otherwise the whole document runs on the TeX engine.
This page lists the classes and packages the native job can load. Loading is not the whole
story: a package listed here may still decline for an option or a command it does not take
yet, and then the document runs on the engine (same output, slower). `rushtex main.tex
--native-gaps` lists what keeps a given document off the native path.

**Keeping this list current.** The names come from the code: `rushtex-gen natlist` prints
every class (`data_class::CLASSES`) and package (`engine_doc::native_package_names`:
`DRIVER_PACKAGES`, the native tables in `data_kernel::PACKAGES`, `NO_EFFECT_PACKAGES` and
`NAMED_PACKAGES`). The unit test `native_packages_doc_names_every_package` fails when a name
there is missing here, so a port that adds a package must add its line. The notes are
written by hand from the code and the journal notes; update the note when a port widens
what a package takes.

As of 2026-11-02: 14 classes, 92 packages.

## Classes

| Class | Takes | Declines |
|---|---|---|
| `article` | `10pt`/`11pt`/`12pt` (an undeclared size is ignored, as article.cls does), the paper sizes, `landscape`, `oneside`/`twoside`, `onecolumn`/`twocolumn`, `draft`/`final`, `titlepage`/`notitlepage` | `leqno`, `fleqn`, `openbib`, unknown options, two options of one group |
| `report` | as article, plus `\chapter`, `\chapter*`, `titlepage`, `openany`, contents | `twocolumn`, as article |
| `book` | as report, `openright`/`openany`, `\cleardoublepage`, front/main/back matter, two-sided marks, page labels | `twocolumn`, as article |
| `letter` | the letter class's fields and closing; `onecolumn`, `notitlepage` ignored | `twoside`, `leqno`, `fleqn` |
| `amsart` | class table plus amsart.cls macros: front matter, abstract, headings, theorem skips, addresses; `landscape`, `draft`, its default options | `oneside`, `reqno`, `tbtags`, `fleqn`, `titlepage`, `twocolumn`, `8pt`, `9pt`, `noamsfonts`, `nomath` |
| `IEEEtran` | class table, its font tuning, hyperref under it; `journal`, `conference`, `twocolumn`, `oneside`, `final` | `9pt`, `onecolumn`, `twoside`, `draft*`, `technote`, `peerreview*`, `compsoc`, `comsoc`, `transmag` |
| `memoir` | paragraphs, sections, chapters, matters, title, contents, `\chapterstyle{default,section}`, `\setsecnumdepth`, `\settocdepth`, `titlingpage`, emulated epigraph; `10pt`-`12pt`, `oneside`/`twoside`, `draft`/`final`, `landscape` | other chapter styles, other sizes, `twocolumn`, `openleft`, `ms`, `showtrims`, `leqno`, `fleqn` |
| `scrartcl` | `10pt`/`11pt`/`12pt`, paper names (not `letterpaper`), `landscape`, `oneside`, `onecolumn`, `notitlepage`; `\setcounter{secnumdepth}` (contents and `\sectionmark` follow it; not with hyperref past the depth) | `DIV=`, `BCOR=`, any `key=value`, `twoside`, `draft`, `twocolumn` |
| `scrreprt` | as scrartcl, with chapters | as scrartcl |
| `scrbook` | as scrartcl, two-sided by default, `openright`/`openany` | as scrartcl |
| `scrlttr2` | letter fields, DIN reference line, location field, `parskip=half` | other `.lco` files and keys |
| `extarticle` | the eight extsizes sizes | `titlepage` |
| `a0poster` | `a0b`, `a0`, `a1`, `a2`, `a3`, `portrait`, `landscape`, `final` | `posterdraft` and any other option |
| `screenplay` | no options | any option |

## Math

| Package | Takes | Declines |
|---|---|---|
| `amsmath` | displays (`equation`, `align`, `gather`, `multline`, `split`, `aligned`, `gathered`), `\tag` (in any `multline` row too, a forward `\ref` to a tag of commands or math, a tag inside a macro of the document's, with microtype and hyperref), matrices, `cases`, `\dfrac`/`\binom`/`\genfrac`, `\text`, operators, `\boldsymbol`, `\allowdisplaybreaks`, `\tag` texts with formulas and commands, `\sideset`, `subequations` (with hyperref and cleveref too); options `centertags`, `namelimits`, `nointlimits`, `sumlimits` | `fleqn`, `leqno`, `intlimits`; `\tag` in `split`, a forward `\ref` to a tag holding a macro of the document's, nested accents, `\shoveleft`/`\shoveright`, `subequations` nested or in math, `\allowdisplaybreaks` in the body |
| `amssymb` | every symbol; `psamsfonts` | |
| `amsfonts` | `\mathbb`, `\mathfrak`, wide accents; `psamsfonts` | |
| `amsthm` | `\theoremstyle` plain/definition/remark, `\newtheorem*`, `proof`, `\qed`, `\qedhere` in a paragraph, `equation*`, `\[ \]`, a numbered `equation` and the last field of an `align*` row | `\qedhere` in `align`, `alignat*`, `flalign*`, `gather*`, an inner formula, an earlier field, under hyperref in `align*`; `\newtheoremstyle`, `\swapnumbers` |
| `amsbsy` | (loaded by amsmath) table | |
| `amsgen` | (loaded by amsmath) table | |
| `amsopn` | (loaded by amsmath) table | |
| `amstext` | (loaded by amsmath, siunitx) table | |
| `mathtools` | loads (with `calc`, `keyval`, `mhsetup`); `dcases`, `rcases`, `drcases` and their starred forms, `cases*`; `matrix*` and `pmatrix*`.. `Vmatrix*` with `[l]`, `[c]`, `[r]`; `\xmapsto`, `\xleftrightarrow`; `\DeclarePairedDelimiterX` (1 to 9 arguments, `\delimsize`) | its own commands (`\coloneqq`, `\DeclarePairedDelimiter`, ...) |
| `mhsetup` | (loaded by mathtools) table | |
| `bm` | `\bm` of math characters | groups, accents, `\pmb`, primes |
| `mathrsfs` | `\mathscr`, family moved after other math fonts | |
| `latexsym` | the `lasy` font and its symbols (`\mho`, `\Join`, `\Box`, `\Diamond`, `\leadsto`, `\sqsubset`, `\sqsupset`, `\lhd`, `\rhd`, `\unlhd`, `\unrhd`) | |
| `dsfont` | `\mathds` | the `sans` option |
| `bbm` | `\mathbbm`, `\mathbbmss`, `\mathbbmtt` | |
| `stmaryrd` | its font, 104 symbols, `\llbracket`/`\rrbracket` and its other delimiters, its arrow macros | `\oast`, `\ocircle`, `\circledast`, `\circledcirc`, `\varcopyright`; options |
| `upgreek` | upright Greek (`\upalpha` .. `\Upomega`) in Euler Roman | the `Symbol` options |
| `esint` | its integrals (`\iint`, `\oiint`, `\fint`, `\sqint`, `\ointctrclockwise`, ...), also after amsmath | options |
| `bbold` | `\mathbb` in the bbold font, also after amssymb | `\textbb`, `\bbfamily` |
| `braket` | `\bra`, `\ket`, `\braket`, `\Bra`, `\Ket`; `\set`, `\Set`, `\Braket` split at their top-level bars | a bar the active `|` reaches in a group, after a command that may read it (`\left|`), or in a macro's text |
| `wasysym` | loads; its math symbols (`\lhd`, `\Box`, `\Diamond`, `\leadsto`, `\mho`, `\Join`, `\ocircle`, `\apprle`, ...) | its text symbols (129 names), `\dh`/`\DH` after it, options |
| `nicefrac` | `\nicefrac` in text and formulas (with `ifthen`) | `\nicefrac[font]`, option `ugly` |
| `units` | `\unit[value]{unit}`, `\unitfrac[value]{num}{den}` (with `nicefrac`) | options `loose`/`ugly`; units with siunitx (both define `\unit`) |
| `cancel` | `\cancel`, `\bcancel`, `\xcancel` in formulas and text, `\cancelto` in formulas (slash and arrow as picture lines); options `makeroom`, `overlap`, `samesize`, `smaller`, `Smaller`, `thicklines` | a slash shorter than its line piece; `\cancelto` in text |
| `adjustbox` | loads (with graphicx, `xkeyval`, `adjcalc`, `collectbox`, `trimclip`, `ifoddpage` with its `.aux` write, `varwidth`; each also loads alone) | options; `\adjustbox`, `\adjincludegraphics` and its other commands and keys |
| `xfrac` | loads (with `amstext`, `graphicx`) | `\sfrac`, `\textfractionsolidus`, `\DeclareInstance` and the other template commands |
| `gensymb` | `\degree`, `\celsius`, `\ohm` (`\ensuremath{^\circ}` and kin); with textcomp their text forms (`\textdegree`, `\textcelsius`, `\textohm`, `\textmu` for `\micro`) | with textcomp, all five in a formula (its `gns@font` family) and `\perthousand`; options |
| `ifthen` | `\newboolean`, `\provideboolean`, `\setboolean`; `\ifthenelse` with `\boolean`, `\equal` (characters and the document's macros without parameters), `\isodd`, `<`, `=`, `>` of integers and `\value` (the document's, the headings', the lists', footnote, figure, table, equation counters), `\NOT`, `\AND`, `\OR` (either case) | `\(` `\)`, `\lengthtest`, `\isundefined`, `\whiledo`, `\value{page}`; a branch that numbers something before a table of contents or a forward reference |

## Tables

| Package | Takes | Declines |
|---|---|---|
| `array` | the preamble automaton: `c l r p m b`, `\|`, `@{}`, `>{}`, `<{}`, `!{}`, `\arraybackslash`, `\newcolumntype` | `\extrarowheight`, `\firsthline` |
| `tabularx` | `X` columns and `tabular*` to the width | counting or labelling bodies |
| `booktabs` | `\toprule`, `\midrule`, `\bottomrule`, `\cmidrule`, `\morecmidrules`, `\specialrule`, `\addlinespace` | setting its lengths, trims with a width |
| `multirow` | `\multirow[c\|t\|b]{n}{*}[move]{text}`, a width (the text as a paragraph) | `=` width, big struts |
| `longtable` | a longtable on one page, with `\rowcolors` | page-breaking longtables |
| `colortbl` | `\rowcolors`, cell colours (also via `xcolor`'s `table`) | some `\arrayrulecolor`/`\columncolor` uses |
| `dcolumn` | loads (table) | the `D` column |
| `makecell` | `\makecell[v h]{a\\b}`, `\thead` (footnotesize, gape) | `\arraystretch` in the document, empty `[]`, `\makegapedcells`, `\Gape`, `\rothead`, `\diaghead` |
| `threeparttable` | `threeparttable`, `tablenotes`, captions | |
| `siunitx` | loads (with amstext, array, colour); literal units, digit groups, exponents; `\ang` (no `;`), `\numrange`, `\SIrange`/`\qtyrange` in text; `\degree`, `\arcminute`, `\arcsecond` units | `S` columns, most keys; ranges in a formula or under a non-English babel language; `\micro` |

## Layout and lists

| Package | Takes | Declines |
|---|---|---|
| `geometry` | its keys (as `\Gm@@process`), `\geometry`, `\newgeometry`, the PDF page size | `hmarginratio` |
| `fancyhdr` | `\pagestyle{fancy}`, `\fancyhf`, `\fancyhead`/`\fancyfoot`, `\lhead`..`\rfoot`, rule widths, marks, two-sided slots, `\nouppercase`, the marks of `\tableofcontents` and `thebibliography` under article and report, `\fancypagestyle{plain}{...}` in the preamble (slot commands and the rule widths) | `\fancypagestyle` of another style, or with other code; article's `thebibliography` marks under babel |
| `titlesec` | `compact`, `big`, `medium`, `small`, `tiny`, `raggedright`, `raggedleft`, `center`, family/series/shape options (and under report and book the display chapter they give); `\titleformat` (hang, block, display and run-in shapes; `\thetitle`, `\titlerule`, `\filcenter` and kin, `\MakeUppercase`, colours), `\titlespacing`, `\titlelabel` (article, report, book), with parskip | frame, wrap, drop and margin shapes; a run-in format in colour, a heading right after a run-in one; `\paragraph` under titlesec; `\titleformat*{\chapter}`, `\part`; under KOMA or memoir |
| `setspace` | `\singlespacing`, `\onehalfspacing`, `\doublespacing`, `\setstretch`, its environments | `singlespace*`; a footnote when setspace is loaded after hyperref |
| `parskip` | no options; with `\tableofcontents`, `\listoffigures`, `\listoftables` (its `\@starttoc` patch) | with amsthm |
| `multicol` | `multicols`, `multicols*`, balancing, page breaks, `\columnbreak` | |
| `enumitem` | keys on `enumerate`/`itemize`, `\setlist` in the preamble (lengths, `nosep`, `leftmargin=*`, `label`); option `shortlabels` | `\newlist`, `align`, `font`, `widest`, `resume`, other options |
| `indentfirst` | indents the first paragraph after a heading | |
| `placeins` | `\FloatBarrier` (no options) | options (`section`, `above`, `below`) |
| `fullpage` | `in`, `cm`, `plain`, `empty`, `myheadings`, `headings` (book): the margins and text block from the class's paper | after geometry (geometry after it is native), under KOMA or memoir; `headings` under article and report |
| `xurl` | no options: letters and digits as break points, `\Urlmuskip` `0mu plus 1mu` | options, after biblatex |
| `chngcntr` | `\counterwithin` and `\counterwithout` of `subsection` or a lower heading counter that nothing in the document steps or prints (unnumbered `\paragraph` headings allowed when no `\label`, hyperref or `secnumdepth` could show them) | the counter or one under it used anywhere; any other counter |
| `lipsum` | loaded with its commands unused (it sets nothing) | options; `\lipsum` and its other commands used |
| `verbatim` | the `verbatim` environment as the kernel's (no text after `\end{verbatim}` on its line, not empty); its `comment` environment with both lines alone, dropped with its lines | `verbatim*`, `\verbatiminput`, `\verbatimwrite`; text after `\end{verbatim}`, which it drops |
| `comment` | the `comment` environment, `\begin{comment}` and `\end{comment}` each alone on its line (an `\end{comment}` after a tab), dropped with its lines | options; `\excludecomment`, `\includecomment`, `\specialcomment`; either line with other text |
| `nth` | `\nth{n}` of a number; `super` | negative numbers, `\ordinal`, `negative` |
| `xspace` | `\xspace` before letters, punctuation, braces, `\par`, `\begin`/`\end` and unexpandable commands | before a macro it would expand |
| `enumerate` | label templates such as `[(i)]`, `[(A). ]` | a template with no counter |
| `etoolbox` | loading it (no options); `\preto`, `\appto`, `\gpreto`, `\gappto` of `\@verbatim` in a document with no verbatim environment (the patch changes nothing) | any other patch target, `\pretocmd`, `\apptocmd`, `\patchcmd`, its other commands |
| `float` | `[H]` with its caption; preamble `\newfloat{type}{placement}{ext}[within]` (within a heading level or a counter), `\floatstyle` `plain`, `plaintop`, `ruled` and `boxed`, `\floatname`, `\floatplacement`, `\restylefloat` of figure, table and new types, their captions (float's own, or caption's box and `ruled` style), with hyperref; `\listof{type}{title}` (one column, no running heads) | `\restylefloat*`, `\floatevery`, a caption continuation `\caption{..}[..]`, a restyled figure or table with caption and hyperref, sub-captions in a restyled float, `\captionsetup[type]` of a new type |
| `algorithm` | options `plain`, `ruled`, `boxed`, `section`, `subsection`, `subsubsection`, `chapter` (report and book), `nothing`, and any other word as the float's name: float.sty's `algorithm` float with its caption (float's or caption's), `\label`/`\ref`, `[H]`, with hyperref ; `\listofalgorithms` | `part` |
| `algorithmicx` | the `algorithmic` environment with `[n]` line numbers, `\State`, `\Statex`, `\Comment`, `\algorithmicindent`, `\alglinenumber`, `\algrenewcommand` and `\algnewcommand` of its keyword macros, `\algtext*` and `\algnotext` of an entity, `\label` on a line (its number) | `\algdef`, `\algblock`, `\algloop` and the other block definitions, `\algtext` with a new text, `\algstore`/`\algrestore`, a `\label` on a line with cleveref |
| `algpseudocode` | `noend` and `end`: `\If`/`\ElsIf`/`\Else`/`\EndIf`, `\For`, `\ForAll`, `\EndFor`, `\While`/`\EndWhile`, `\Loop`/`\EndLoop`, `\Repeat`/`\Until`, `\Procedure`, `\Function` and their ends, `\Require`, `\Ensure`, `\Return`, `\Call`, `\textproc`, the `\algorithmic..` keyword macros | `compatible` (algcompatible's `\STATE` forms) |
| `caption` | no options and the common keys (`font`, `labelfont`, `textfont`, `labelsep`, `justification`, `singlelinecheck`, `position`, skips, `format=hang`, `labelformat` simple, empty and parens, `margin`, `width`, `indention`) as options and preamble `\captionsetup`, also `\captionsetup[figure]` and `[table]`, under article, report and book; with hyperref (`hypcap` anchors); in float.sty `[H]` floats; `\captionof` outside floats; listings' captions (caption3's box); with longtable, threeparttable and listings loaded | other keys, `\captionsetup[type]` of other types, body `\captionsetup`, `\caption*`, `\captionof*`, `\captionof` with hyperref, a `\caption` in a longtable or a threeparttable, a listing caption in a list |
| `capt-of` | `\captionof{figure}` and `\captionof{table}` outside floats (the kernel's caption, in its list of figures or tables) | `\captionof` with hyperref, under KOMA or memoir, other types |
| `subcaption` | no options: `subfigure`, `subtable`, sub-captions `(a)`, their labels and `\subref`, also ahead of the figure | options, `\captionsetup[sub]`, `\subcaption`, `\subcaptionbox`, `\subref*` |
| `subfig` | no options: `\subfloat[caption]{body}` and `\subfloat[list][caption]{body}` in a figure or table with the main caption below, caption3's sub-caption box (footnotesize, `(a)`), `\label` in the body and `\subref`, also ahead of the figure | options (`caption=false`), hyperref, lists of figures, a caption above, `\ContinuedFloat`, `\subref*`, caption margins |
| `footmisc` | `bottom` with `hang`, `symbol`, `symbol*`, `flushmargin`, `marginal`, `norule`, `multiple`, `stable` | without `bottom`; footnotes and floats with it |
| `lastpage` | `\pageref{LastPage}`, with hyperref too | |
| `pagecolor` | no options: white page colour | options |
| `ragged2e` | under scrlttr2 only | elsewhere |
| `microtype` | no options: protrusion per font, its patches | options, `\microtypesetup` |
| `authblk` | `\author[mark]`, `\affil` in the preamble | its parameters (`\Authsep`, `\affilsep`, ...) |
| `epigraph` | memoir's emulated package only | the real epigraph.sty |
| `tcolorbox` | standard skin, options, `\newtcolorbox`, `\tcbset`, `[most]`, multicols in a box | other skins and libraries |

## Fonts and encodings

| Package | Takes | Declines |
|---|---|---|
| `fontenc` | `[T1]` | any other encoding, several encodings |
| `inputenc` | `[utf8]` (a no-op: LaTeX's default) | other encodings |
| `lmodern` | table: Latin Modern text and math | unknown text families |
| `textcomp` | table | |
| `helvet` | no options (`\sfdefault` phv) | `scaled` |
| `courier` | `\ttdefault` pcr | |
| `times` | ptm, phv, pcr | |
| `palatino` | ppl, phv, pcr | |
| `mathpazo` | table: Palatino text, Pazo math fonts, its math codes, `\medmuskip` and math sizes; `sc` | `osf`, `slantedGreek`, `noBBpl`; `\mathbb`, `\big` and kin; with amsfonts or amssymb; after other math fonts |
| `babel` | english, american, british, brazilian, portuguese, spanish, ngerman, german, italian, french (main or secondary), captions, dates, patterns, `\selectlanguage`, `\foreignlanguage` | other languages, babel loaded twice |

## Graphics and colour

| Package | Takes | Declines |
|---|---|---|
| `graphicx` | `\includegraphics` of PDF and PNG with `width`, `height`, `scale`, `keepaspectratio`, `angle`, `trim`/`viewport`, `clip`; `\graphicspath`, `\scalebox`, `\resizebox(*)`, `\rotatebox`; options `pdftex`, `final` | JPEG, `scale` with `angle`, `origin`, `page`, `\reflectbox` |
| `graphics` | graphicx's code without its keys: `\includegraphics{file}`, `\scalebox`, `\resizebox`, `\rotatebox`; with graphicx, tikz or mathtools loaded it is graphicx | `\includegraphics[llx,lly][urx,ury]`, `\rotatebox[..]` without graphicx |
| `color` | `\color`, `\textcolor`, `\definecolor`; `usenames`, `dvipsnames`, `pdftex` | `\colorbox` from color.sty |
| `xcolor` | names, rgb/gray/cmyk/RGB/HTML, mixes `c!p!c2`, cmyk mixes, `\colorbox`, `\fcolorbox`; `usenames`, `dvipsnames`, `svgnames`, `pdftex`, `table` | complements `-c`, `>wheel` |
| `tikz` | paths, arcs, grids, `\foreach`, nodes (rectangle, circle, anchors, `of`, labels, on paths), curves, `plot[smooth]`, arrow tips incl. `stealth` and the `arrows` library's `latex` and `stealth'`, `>=` tips (after `->` too), the picture's or a scope's `shorten <`/`>`, `auto` in pictures without path labels, rounded corners, opacity, `xscale`/`yscale`, page frames with `remember picture,overlay`, `overlay` pictures without `remember picture` (no page points), preamble `\tikzset` styles and `\tikzstyle`, a point with nodes (`\draw (p) node`), several option brackets, kernel symbols in node formulas, `edge` (labels, several from a point, placement keys for its labels), `scope`, calc sums of points, `\includegraphics` in nodes, the preamble's `every picture`, names with spaces, `em` minimum sizes, `text centered`, one-support curves, trees (`child`, grow down, `level distance`, `sibling distance`, `level`/`level <n>` styles of those, `transform shape` under the identity, `grow=` in the picture's options, `edge from parent` with options and labels), `\tikzset` and `\tikzstyle` styles inside a picture or scope, `to` without options, `to`/`edge` with `bend left`, `bend right`, `out`/`in` (between points or nodes, tips and labels too), `looseness`, `min distance`, `shorten <`/`>`, edge loops (`loop above` ..), `every loop/.style={}`, `\path[options]` with edges (labels under width options), quotes in node text (not under babel), `\strut`, unitless `inner sep`, a circle's `base`, `baseline=<length>` (pt, ex, em), `\tikz[opts]{..}`, blank lines between commands; libraries `positioning`, `calc`, `arrows`, `arrows.meta`, `topaths`, `shapes.geometric`, `calendar` (loaded) | `auto` with path labels, most arrows.meta tips, most libraries, a `to` loop outside an edge, coloured scopes, node `scale=`, `\tikz{..}` without options, trees with child options, `grow` in a style, labels on an edge to a child with children, or a scale, pgfplots |
| `pgf` | (loaded by tikz) table | |
| `pgfcore` | (loaded by tikz) table | |
| `pgfrcs` | (loaded by tikz) table | |
| `pgfsys` | (loaded by tikz) table | |
| `pgfkeys` | (loaded by tikz) table | |
| `pgfmath` | (loaded by tikz) table | |
| `pgffor` | (loaded by tikz) table | |
| `pgfcomp-version-0-65` | (loaded by tikz) table | |
| `pgfcomp-version-1-18` | (loaded by tikz) table | |
| `qrcode` | `\qrcode` (loads xcolor) | |

## References and links

| Package | Takes | Declines |
|---|---|---|
| `hyperref` | anchors (`eqnarray`'s one `equation.N` target, its rows' labels pointing there), `\ref`/`\pageref`/`\eqref`/`\cite` links, `\url`, `\href` (its URL with `#` and a fragment, `~`, `\#`, `\%`), `\tag` targets (`AMS.N`, twice per tag in `align` and `gather`), captions of floats numbered `\counterwithin` a heading, outlines, document info, `hidelinks`, `colorlinks`, link colours, `pdfborder`, `pdfstartview`, `unicode`, ... | `bookmarks*` keys, `breaklinks`, `backref`, `pdfpagemode`, `hyperfootnotes`, `plainpages`, `pdfusetitle`; `\href` options |
| `cleveref` | with or without hyperref: `\cref`, `\Cref`, `\crefrange`, `\Crefrange`, `\labelcref` (sorted and compressed lists of one type); the type is the counter `\refstepcounter` stepped (the kernel's first aid), so equations, figures, tables, headings, items, theorems (by their counter) and the document's own counters; English names, `\crefname`/`\Crefname`, a theorem's title as its singular name; options `capitalise` (`capitalize`), `noabbrev`, `nameinlink` | lists mixing types; a plural of a type with no plural name; a label after `\tag`; a reference ahead to an item; starred references; other options |
| `csquotes` | `\enquote`, `\enquote*`, nested, with their space factors, markers and kerns (OT1 and T1); the American style, or with babel English's American and British; options `autostyle`, `babel`, `strict` | `\textquote`, `\blockquote`, `displayquote`, `\MakeOuterQuote`, `\foreignquote`; other languages' styles; `\frenchspacing`; a font switch left open in a quotation; a macro or a ligature next to a mark |
| `url` | `\url`, `\path`, `\urlstyle` | options |
| `cite` | no options: `\cite` sorted, runs of three or more compressed with a dash, the space before adjusted, `[note]`, undefined keys' `?`; `\nocite`; with hyperref and bibtex | options; labels other than plain numbers; a key cited twice in one `\cite`; `\citen`, `\citenum` |
| `natbib` | `\citep`, `\citet` and kin, `\bibpunct`, `\setcitestyle`; `round`, `square`, `curly`, `angle`, `numbers`, `super`, `sort`, `compress`, `comma`, `semicolon`, ...; a `.bbl` | with biblatex |
| `apacite` | `\cite`, `\citeNP`, `\citeA`, `\citeauthor`, `\citeauthorNP`, `\citeyear`, `\citeyearNP` and their `full`/`short` forms, with a note `[..]`, for keys no `.bbl` defines (`(?, ?)`); the punctuation macros `\BBOP`, `\BBAY`, ... as the package defines them | options; a prefix `<..>`; a `.bbl`; with hyperref, natbib, backref, bibunits, babel or url |
| `biblatex` | numeric, authoryear, alphabetic styles with no `.bbl` (missing-key citations, empty `\printbibliography`) | a `.bbl`, apa/ieee/chicago styles |
| `makeidx` | `\makeindex`, `\index`, `\see`, `\printindex` (no index file) | |

## Other

| Package | Takes | Declines |
|---|---|---|
| `listings` | `lstlisting`, `\lstinline`, `\lstinputlisting`, preamble `\lstset` (fixed/fullflexible columns, frames, `breaklines`, captions, comments, strings, Python keywords) | other languages' keyword sets, uncommon keys |
| `calc` | table: `\setlength` expressions | |
| `keyval` | table | |
