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
| **07-DELIVERY-PHASES.md** | مراحل التنفيذ الـ 8 من Phase 0 إلى Phase 7 مع الـ Gates ومعايير القبول لكل مرحلة. | [`07-DELIVERY-PHASES.md`](./07-DELIVERY-PHASES.md) |
| **DESIGN.md** | نظام التصميم ومواصفات Supabase Dark Infrastructure وألوان الرموز والخطوط المستخرجة من Stitch. | [`DESIGN.md`](./DESIGN.md) |

---

## 🎯 كيفية استخدام هذه الحزمة مع النماذج الأخرى:
1. انسخ محتوى البرومبت في [`06-INDEPENDENT-AI-REVIEW-PROMPT.md`](./06-INDEPENDENT-AI-REVIEW-PROMPT.md).
2. أرفق معه الملفات من `00` إلى `05` (أو الصق محتواها).
3. سيقوم الموديل الآخر بفحص كل زاوية معمارية (Clean Architecture، أداء الرادار، دقة افتراضات Jev، ومعالجة الـ Edge Cases) وإعطائك تقريراً تقييمياً بنقاط الضعف والتوصيات قبل البدء في كتابة أي سطر كود.
