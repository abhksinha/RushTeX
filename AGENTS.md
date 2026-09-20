# RUSHTEX Agent Guidelines
For each user prompt, internally classify if user is asking question or he is asking you to work on his codebase. If he is asking you to work then follow **Reporting** and if he is asking architecture related work you need to communicate to user as per **General Answers**. 
## Communication Style & Reporting
- **Executive Summaries in Brevity**: Always provide concise, high-level executive summaries; but work in depth. 
- **No Unsolicited Code Details**: Do not include code snippets, method signatures, file diff breakdowns, or implementation minutiae in final responses unless the user explicitly requests technical details.
- **Work Style**: Use `graphify` repository knowledge tool for understanding project. Read only what is required, and move function by function.  
- **Follow**: Follow god_instruction.md along bit packing of data structure with suitable Structure of Arrays (SoA) and Arrays of Structure (AoS) wherever applicable so that L1, L2, L3 cache of CPU is utilised in most efficient way possible.
- **Math Typsetting**: Use RushTeX_Math_Architecture_Blueprint.md to steer the work for mathematical typesetting.
- **Reporting**: If user is not asking question, and has instructed you to work on something or your are executing any code changes and you want to report then follow:
  1. What the issue / task was
  2. What was done at a high level
  3. Was it most optimised way to utilised L1, L2 and L3 cache, and it not why not. Reply "Not Applibale if there was no changaes made."
  4. Recommendation cum permission to / from user in order to implement the work demand of user in most optimised way.
- **General Answers**: When user asks your question, answer in simplified manner in brevity with balanced paragraphs and enumerations.
