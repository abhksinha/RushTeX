%%
%% This is file `captiondoc.cls',
%% a common document class for all caption package bundle documentations.
%%
%% There is no need to distribute this file in tex/latex/caption since
%% it's of no use for the end-user, it's only needed to compile the package
%% documentations.
%%
%% Copyright (C) 2023 Axel Sommerfeldt (axel.sommerfeldt@f-m.fm)
%%
%% https://gitlab.com/axelsommerfeldt/caption
%%
%% --------------------------------------------------------------------------
%%
%% This work may be distributed and/or modified under the
%% conditions of the LaTeX Project Public License, either version 1.3
%% of this license or (at your option) any later version.
%% The latest version of this license is in
%%   http://www.latex-project.org/lppl.txt
%% and version 1.3 or later is part of all distributions of LaTeX
%% version 2003/12/01 or later.
%%
%% This work has the LPPL maintenance status "maintained".
%%
%% This Current Maintainer of this work is Axel Sommerfeldt.
%%
%% This work consists of the files
%%   caption.ins, caption.dtx, caption-light.dtx, caption2.dtx, caption3.dtx,
%%   caption-ams-smf.dtx, caption-beamer.dtx, caption-elsarticle.dtx,
%%   caption-koma.dtx, caption-memoir.dtx, caption-ntg.dtx,
%%   caption-thesis.dtx, bicaption.dtx, ltcaption.dtx, subcaption.dtx,
%% the derived files
%%   caption.sty, caption-light.sty, caption2.sty, caption3.sty,
%%   caption-ams-smf.sto, caption-beamer.sto, caption-elsarticle.sto,
%%   caption-koma.sto, caption-memoir.sto, caption-ntg.sto,
%%   caption-thesis.sto, bicaption.sty, ltcaption.sty, subcaption.sty.

\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{captiondoc}[2022/12/27 Document class used by caption documentation]

\hbadness=9999 \newcount\hbadness \hfuzz=100pt % Make TeX shut up.

\PassOptionsToPackage{breaklinks=true}{hyperref}

\LoadClassWithOptions{ltxdoc}
\setlength\parindent{0pt}
\setlength\parskip{\smallskipamount}
\addtolength\marginparwidth{15pt}
\setlength\leftmargini{2em}% default = 2.5em

\renewcommand\partname{Part}

\RequirePackage{ifpdf}
\ifpdf
  \RequirePackage{mathptmx,courier}
  \RequirePackage[scaled=0.90]{helvet}
\fi

\RequirePackage{hypdoc}
\ifpdf\RequirePackage{hypdestopt}\fi
\hypersetup{pdfstartpage={},pdfstartview={}}
\hypersetup{breaklinks=true}
\let\subsectionautorefname\sectionautorefname
\let\subsubsectionautorefname\sectionautorefname

\DeclareRobustCommand*\eTeX{\leavevmode\hbox{$\varepsilon$}-\TeX}

\DeclareRobustCommand*\AmS{\texorpdfstring
  {{\protect\usefont{OMS}{cmsy}{m}{n}A\kern-.1667em\lower.5ex\hbox{M}\kern-.125emS}}%
  {AMS}}
\DeclareRobustCommand*\KOMAScript{\textsf{K\kern.05em O\kern.05em M\kern.05em A\kern.1em-\kern.1em Script}}
\DeclareRobustCommand*\NTG{NTG}
\DeclareRobustCommand*\SmF{SMF}

\newcommand*\NEWfeature{\NEW{New feature}}
\newcommand*\NEWdescription{\NEW{New description}}
\newcommand*\NEW[1]{\@ifstar{\@NEW{#1}{\vskip2pt}}{\@NEW{#1}{}}}
\newcommand*\@NEW[3]{\MARGINSYM{#2\footnotesize#1\\#3}}
\newcommand*\MARGINSYM[1]{\hskip 1sp \marginpar{\raggedleft\textcolor{blue}{{#1}}}}

\newcommand*\LineBreak{\linebreak[3]}
\newcommand*\PageBreak{\pagebreak[3]}

\newcommand*\longref{\@ifstar{\@longref\ref}{\@longref\autoref}}
\newcommand*\Longref{\@ifstar{\@longref\Ref}{\@longref\Autoref}}
\def\@longref#1#2{#1{#2}: \textit{\nameref{#2}}}

\newcommand*\See[1]{\nopagebreak{\small (See #1)}}

\newenvironment{Options}[1]%
  {\list{}{\renewcommand\makelabel[1]{\texttt{##1}\hfil}%
   \settowidth\labelwidth{\texttt{#1\space}}%
   \setlength\leftmargin{10pt}%
   \addtolength\leftmargin{\labelwidth}%
   \addtolength\leftmargin{\labelsep}}}%
  {\endlist}

\newcommand*\purerm[1]{{\upshape\mdseries\rmfamily #1}}
\newcommand*\puresf[1]{{\upshape\mdseries\sffamily #1}}
\newcommand*\purett[1]{{\upshape\mdseries\ttfamily #1}}
\let\cls\puresf \let\pkg\puresf
\let\env\purett \let\opt\purett

\newcommand*\csmarg[1]{\texttt{\char`\{#1\char`\}}}
\newcommand*\csoarg[1]{\texttt{\char`\[#1\char`\]}}
\newcommand*\issue[2][GitLab]{#1 issue~\##2}
\newcommand*\version[2][]{v$#2$}
\newcommand*\changenote[1]{}

\RequirePackage{marvosym}
\newcommand*\INFO{\@ifstar{\@INFO{}}{\@INFO{\vbox to \ht\strutbox}}}
\newcommand*\@INFO[1]{\MARGINSYM{#1{\LARGE\Info}}}

\RequirePackage[alpine]{ifsym}
\newenvironment{background}{\par\bigskip\csname background*\endcsname}{\csname endbackground*\endcsname}
\newenvironment{background*}{\small\MARGINSYM{\Mountain}\ignorespaces}{\par}

\RequirePackage[bottom]{footmisc}

\endinput

