# 📁 CodeRadar Studio: Architectural Specification Package

مجموعة وثائق المواصفات الهندسية والمعمارية المتكاملة لمشروع **CodeRadar Desktop Studio**، جاهزة بالكامل لتقديمها إلى أي نموذج ذكاء اصطناعي آخر لإجراء مراجعة نقدية مستقلة (Adversarial Architectural Review) قبل كتابة الكود.

---

## 📑 فهرس وثائق الحزمة (Specification Documents)

| الملف | الوصف ومحور التقييم | الرابط المباشر |
| :--- | :--- | :--- |
| **00-EXECUTIVE-SUMMARY.md** | الملخص التنفيذي، الرؤية العامة، والمقارنة مع أدوات المراجعة التقليدية. | [`00-EXECUTIVE-SUMMARY.md`](./00-EXECUTIVE-SUMMARY.md) |
| **01-BRD-AND-MVP-REQUIREMENTS.md** | وثيقة متطلبات العمل، شخصيات المستخدمين، ونطاق الـ MVP مقابل الإصدارات القادمة. | [`01-BRD-AND-MVP-REQUIREMENTS.md`](./01-BRD-AND-MVP-REQUIREMENTS.md) |
| **02-CLEAN-ARCHITECTURE-BLUEPRINT.md** | المخطط المعماري الصارم، وتطبيق The Dependency Rule ومبادئ SOLID عبر Rust و Tauri 2.0. | [`02-CLEAN-ARCHITECTURE-BLUEPRINT.md`](./02-CLEAN-ARCHITECTURE-BLUEPRINT.md) |
| **03-JEV-DECISION-ENGINE-INTEGRATION.md** | التفاصيل الهندسية لتكامل نموذج Jev، وهياكل الـ Schemas، وخط أنابيب الـ Dual-Tier، والـ Mock Engine. | [`03-JEV-DECISION-ENGINE-INTEGRATION.md`](./03-JEV-DECISION-ENGINE-INTEGRATION.md) |
| **04-UNIFIED-REVIEW-SKILLS-MATRIX.md** | استخلاص "الزبدة" وإزالة التكرار من المهارات الـ 6 عبر 5 ركائز تطابق أزرار الواجهة. | [`04-UNIFIED-REVIEW-SKILLS-MATRIX.md`](./04-UNIFIED-REVIEW-SKILLS-MATRIX.md) |
| **05-UI-UX-AND-POLAR-RADAR-SPEC.md** | المواصفات الرياضية والفيزيائية لرادار الـ 360° Polar Canvas، وهيكلة شاشة Stitch UI. | [`05-UI-UX-AND-POLAR-RADAR-SPEC.md`](./05-UI-UX-AND-POLAR-RADAR-SPEC.md) |
| **06-INDEPENDENT-AI-REVIEW-PROMPT.md** | البرومبت المخصص لتغذية هذه الملفات لأي نموذج ذكاء اصطناعي آخر لطلب مراجعته المعمارية. | [`06-INDEPENDENT-AI-REVIEW-PROMPT.md`](./06-INDEPENDENT-AI-REVIEW-PROMPT.md) |
| **07-DELIVERY-PHASES.md** | مراحل التنفيذ الـ 9 من Phase 0 إلى Phase 8 مع الـ Gates ومعايير القبول لكل مرحلة. | [`07-DELIVERY-PHASES.md`](./07-DELIVERY-PHASES.md) |
| **08-PHASE-0-SPIKE-AND-DECISIONS.md** | نتائج Spike قياس زمن استجابة Jev من الشبكة الحقيقية، قرار الـ SLA (Tier 1)، اختيار لغات v1.0، وتصحيحات تناسق الوثائق. | [`08-PHASE-0-SPIKE-AND-DECISIONS.md`](./08-PHASE-0-SPIKE-AND-DECISIONS.md) |
| **09-PHASE-2-SCANNER-AND-MOCK-ENGINE.md** | مُخرجات Phase 2: الماسح المتوازي، محرك القواعد التجريبي (Mock)، حدود التغطية، وأدلة بوابة المرحلة. | [`09-PHASE-2-SCANNER-AND-MOCK-ENGINE.md`](./09-PHASE-2-SCANNER-AND-MOCK-ENGINE.md) |
| **10-PHASE-3-TAURI-SHELL-AND-DASHBOARD.md** | مُخرجات Phase 3: هيكل Tauri 2.0، لوحة المعلومات، شريط المقاييس، وعقود الـ IPC. | [`10-PHASE-3-TAURI-SHELL-AND-DASHBOARD.md`](./10-PHASE-3-TAURI-SHELL-AND-DASHBOARD.md) |
| **11-PHASE-4-POLAR-RADAR-CANVAS.md** | مُخرجات Phase 4: طبقات الـ Canvas، إسقاط الإحداثيات القطبية، الثبات 60 إطاراً/ثانية، وأدلة البوابة. | [`11-PHASE-4-POLAR-RADAR-CANVAS.md`](./11-PHASE-4-POLAR-RADAR-CANVAS.md) |
| **12-PHASE-5-JEV-TIER1-AND-PRIVACY-LAYER.md** | مُخرجات Phase 5: محرك Jev من الفئة الأولى، طبقة إخفاء الأسرار قبل الإرسال، وتأخير الاستجابة المقيس. | [`12-PHASE-5-JEV-TIER1-AND-PRIVACY-LAYER.md`](./12-PHASE-5-JEV-TIER1-AND-PRIVACY-LAYER.md) |
| **13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md** | مُخرجات Phase 6: نافذة الفروقات، نموذج الرقعة الآمن (SHA-256 والكتابة الذرّية)، وسجلّ المسح في SQLite. | [`13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md`](./13-PHASE-6-DIFF-MODAL-SAFE-PATCHING-HISTORY.md) |
| **14-PHASE-7-SECURITY-REVIEW.md** | مراجعة أمان تطبيق سطح المكتب مقابل NFR-05: حراسة تجاوز المسارات، تشديد سياسة CSP، وتخزين المفتاح في مخزن مفاتيح نظام التشغيل — مع سبعة ثوابت مفروضة آليًا. | [`14-PHASE-7-SECURITY-REVIEW.md`](./14-PHASE-7-SECURITY-REVIEW.md) |
| **15-PHASE-7-HARDENING-PACKAGING-RELEASE.md** | توثيق المرحلة السابعة: تصدير تقارير Markdown، وميزانية الأداء المقيسة، وحارس الأمان، ومصفوفة إصدارات Windows و macOS و Linux، وبوابة التحقق. | [`15-PHASE-7-HARDENING-PACKAGING-RELEASE.md`](./15-PHASE-7-HARDENING-PACKAGING-RELEASE.md) |
| **DESIGN.md** | نظام التصميم ومواصفات Supabase Dark Infrastructure وألوان الرموز والخطوط المستخرجة من Stitch. | [`DESIGN.md`](./DESIGN.md) |

---

## 🎯 كيفية استخدام هذه الحزمة مع النماذج الأخرى:
1. انسخ محتوى البرومبت في [`06-INDEPENDENT-AI-REVIEW-PROMPT.md`](./06-INDEPENDENT-AI-REVIEW-PROMPT.md).
2. أرفق معه الملفات من `00` إلى `05` (أو الصق محتواها).
3. سيقوم الموديل الآخر بفحص كل زاوية معمارية (Clean Architecture، أداء الرادار، دقة افتراضات Jev، ومعالجة الـ Edge Cases) وإعطائك تقريراً تقييمياً بنقاط الضعف والتوصيات قبل البدء في كتابة أي سطر كود.
