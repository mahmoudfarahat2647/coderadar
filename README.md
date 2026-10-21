# 🛡️ AI Agent Code Review Skills Collection

مجموعة شاملة ومنظمة لأقوى مهارات مراجعة الكود (Code Review Skills) المفتوحة المصدر للمطورين ولوكلاء الذكاء الاصطناعي (AI Coding Agents مثل Claude Code, Antigravity, OpenCode, Cursor, وغيرها).

> ℹ️ **منتج CodeRadar Studio:** هذا الملف هو فهرس لمهارات مراجعة الكود المنسوخة من جهات خارجية فقط، وليس وثيقة المنتج. وثائق المنتج (النظرة المعمارية، دليل التثبيت، طريقة استخدام واجهة سطر الأوامر، ميزانية الأداء، والحدود المعروفة) في [`docs/README.md`](./docs/README.md).

---

## 📂 فهرس الملفات والمجلدات

تم تنزيل جميع المهارات بالتوازي (in parallel) مع توفيرها بصيغتين:
1. **ملفات `.md` مباشرة ومستقلة** في المجلد الرئيسي لسهولة القراءة والنسخ السريع.
2. **مجلدات كاملة لكل مهارة** تتضمن كود `SKILL.md` وملفات المراجع (references)، القوالب (templates)، وقواعد اللغات المختلفة.

---

### 1. Matt Pocock - Code Review
- **المستودع الأصلي**: [`mattpocock/skills`](https://github.com/mattpocock/skills)
- **الملف المباشر**: [`01-mattpocock-code-review.md`](file:///d:/review/01-mattpocock-code-review.md)
- **المجلد الكامل**: [`mattpocock-code-review/`](file:///d:/review/mattpocock-code-review/)
  - [`SKILL.md`](file:///d:/review/mattpocock-code-review/SKILL.md)
  - [`docs/code-review.md`](file:///d:/review/mattpocock-code-review/docs/code-review.md)
  - [`agents/openai.yaml`](file:///d:/review/mattpocock-code-review/agents/openai.yaml)
- **الوصف**: مهارة متخصصة للمهندسين لفحص التعديلات والـ Pull Requests عبر محورين رئيسيين:
  - مطابقة المتطلبات الفنية والمواصفات (Spec Compliance).
  - تغطية حالات الحافة (Edge Cases) واكتشاف السلوكيات غير المتوقعة قبل الدمج.
- **أمر التثبيت السريع**:
  ```bash
  npx skills add https://github.com/mattpocock/skills --skill code-review
  ```

---

### 2. Awesome Skills - Comprehensive Code Review
- **المستودع الأصلي**: [`awesome-skills/code-review-skill`](https://github.com/awesome-skills/code-review-skill)
- **الملف المباشر**: [`02-awesome-skills-code-review.md`](file:///d:/review/02-awesome-skills-code-review.md)
- **المجلد الكامل**: [`awesome-skills-code-review/`](file:///d:/review/awesome-skills-code-review/)
  - [`SKILL.md`](file:///d:/review/awesome-skills-code-review/SKILL.md)
  - [`README.md`](file:///d:/review/awesome-skills-code-review/README.md)
  - [`assets/pr-review-template.md`](file:///d:/review/awesome-skills-code-review/assets/pr-review-template.md)
  - [`assets/review-checklist.md`](file:///d:/review/awesome-skills-code-review/assets/review-checklist.md)
  - **مجلد المراجع `reference/`** (يغطي أكثر من 30 لغة وإطار عمل، بما في ذلك React 19, TypeScript, Rust, Python, Go, C++, Angular, Vue, FastAPI, Django, Dart, وغيرها، بالإضافة إلى أدلة الأمان وأداء الاستعلامات N+1 ومكافحة SQLi و XSS).
- **الوصف**: مهارة مراجعة شاملة تركز على جودة الكود، الأمان، الأنماط المعمارية، وتدعم أحدث الـ Tech Stacks.

---

### 3. Jesse Vincent (obra) - Superpowers (Receiving & Requesting Review)
- **المستودع الأصلي**: [`obra/superpowers`](https://github.com/obra/superpowers)
- **الملفات المباشرة**:
  - [`03-obra-receiving-code-review.md`](file:///d:/review/03-obra-receiving-code-review.md)
  - [`03-obra-requesting-code-review.md`](file:///d:/review/03-obra-requesting-code-review.md)
- **المجلد الكامل**: [`obra-superpowers/`](file:///d:/review/obra-superpowers/)
  - [`receiving-code-review/SKILL.md`](file:///d:/review/obra-superpowers/receiving-code-review/SKILL.md)
  - [`requesting-code-review/SKILL.md`](file:///d:/review/obra-superpowers/requesting-code-review/SKILL.md)
  - [`requesting-code-review/code-reviewer.md`](file:///d:/review/obra-superpowers/requesting-code-review/code-reviewer.md)
- **الوصف**:
  - `receiving-code-review`: ترشد الوكيل الذكي لكيفية استقبال ملاحظات المراجعة من المطورين، والتحقق التقني منها بدقة خطوة بخطوة قبل تطبيق أي تعديل، ومنع التعديل العشوائي أو التراجع عن متطلبات سابقة.
  - `requesting-code-review`: بروتوكول طلب مراجعة الكود من المطور أو من وكيل خبير قبل الدمج.

---

### 4. GitHub Awesome Copilot - Review and Refactor
- **المستودع الأصلي**: [`github/awesome-copilot`](https://github.com/github/awesome-copilot)
- **الملف المباشر**: [`04-copilot-review-and-refactor.md`](file:///d:/review/04-copilot-review-and-refactor.md)
- **المجلد الكامل**: [`copilot-review-and-refactor/`](file:///d:/review/copilot-review-and-refactor/)
  - [`SKILL.md`](file:///d:/review/copilot-review-and-refactor/SKILL.md)
- **الوصف**: مهارة مدمجة لمراجعة الكود وإعادة هيكلته (Refactoring) وتنظيفه وفق أفضل ممارسات Clean Code، مع مراعاة مبادئ SOLID وقابلية الصيانة والاختبار.

---

### 5. Alibaba - Open Code Review (Enterprise Hybrid)
- **المستودع الأصلي**: [`alibaba/open-code-review`](https://github.com/alibaba/open-code-review)
- **الملفات المباشرة**:
  - [`05-alibaba-open-code-review.md`](file:///d:/review/05-alibaba-open-code-review.md)
  - [`05-alibaba-open-code-review-delegate.md`](file:///d:/review/05-alibaba-open-code-review-delegate.md)
- **المجلد الكامل**: [`alibaba-open-code-review/`](file:///d:/review/alibaba-open-code-review/)
  - [`skills/open-code-review/SKILL.md`](file:///d:/review/alibaba-open-code-review/skills/open-code-review/SKILL.md)
  - [`skills/open-code-review-delegate/SKILL.md`](file:///d:/review/alibaba-open-code-review/skills/open-code-review-delegate/SKILL.md)
  - [`commands/review.md`](file:///d:/review/alibaba-open-code-review/commands/review.md)
  - [`commands/delegate-review.md`](file:///d:/review/alibaba-open-code-review/commands/delegate-review.md)
  - [`qca/system-prompt.md`](file:///d:/review/alibaba-open-code-review/qca/system-prompt.md)
  - [`rule.json`](file:///d:/review/alibaba-open-code-review/rule.json)
  - **مجلد القواعد `rules/`** (قواعد تدقيق محددة لأكثر من 50 لغة وتقنية).
- **الوصف**: إطار عمل هجين لمراجعة الكود على مستوى المؤسسات والمشاريع الضخمة، يجمع بين الفحص الثابت الدقيق (Static Analysis) ونماذج الذكاء الاصطناعي مع إمكانية تفويض المراجعات لـ sub-agents.

---

### 6. Spillwave Solutions - PR Reviewer Skill
- **المستودع الأصلي**: [`SpillwaveSolutions/pr-reviewer-skill`](https://github.com/SpillwaveSolutions/pr-reviewer-skill)
- **الملف المباشر**: [`06-spillwave-pr-reviewer.md`](file:///d:/review/06-spillwave-pr-reviewer.md)
- **المجلد الكامل**: [`spillwave-pr-reviewer/`](file:///d:/review/spillwave-pr-reviewer/)
  - [`SKILL.md`](file:///d:/review/spillwave-pr-reviewer/SKILL.md)
  - [`README.md`](file:///d:/review/spillwave-pr-reviewer/README.md)
  - [`references/gh_cli_guide.md`](file:///d:/review/spillwave-pr-reviewer/references/gh_cli_guide.md)
  - [`references/review_criteria.md`](file:///d:/review/spillwave-pr-reviewer/references/review_criteria.md)
  - [`references/scenarios.md`](file:///d:/review/spillwave-pr-reviewer/references/scenarios.md)
  - [`references/troubleshooting.md`](file:///d:/review/spillwave-pr-reviewer/references/troubleshooting.md)
  - [`scripts/generate_review_files.py`](file:///d:/review/spillwave-pr-reviewer/scripts/generate_review_files.py)
  - [`scripts/fetch_pr_data.py`](file:///d:/review/spillwave-pr-reviewer/scripts/fetch_pr_data.py)
  - [`scripts/add_inline_comment.py`](file:///d:/review/spillwave-pr-reviewer/scripts/add_inline_comment.py)
- **الوصف**: مهارة متخصصة في سحب بيانات الـ Pull Requests تلقائياً عبر `gh` CLI، ومراجعتها معمارياً وأمنياً مع قوالب تفصيلية وتعليقات سطرية (Inline Comments).

---

## 🚀 كيفية استخدام المهارات مع وكيلك (AI Agent)

1. **في Claude Code**:
   - انسخ مجلد المهارة المطلوب وضعه داخل `.claude/skills/<skill-name>/` في مشروعك أو في المسار العام `~/.claude/skills/`.
2. **في Antigravity**:
   - ضع المهارة داخل مجلد `.gemini/antigravity/skills/<skill-name>/SKILL.md` أو داخل مسار المشروع.
3. **في OpenCode / Cursor / Windsurf**:
   - يمكنك الإشارة مباشرة لأي ملف `SKILL.md` أو نسخه في توجيهات الـ Prompt / Rules.
