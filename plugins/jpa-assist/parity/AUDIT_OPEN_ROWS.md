# Audit of the open ledger rows

Audited 2026-11-12 against `requirements.json`, `claims/` and the L1 evidence: **223 open rows** of 355 (132 verified: 81 documentation-derived, 51 inherited; 0 observed, 0 source-reading).

## Totals

| Category | Rows | Can headless work close it? |
|---|---|---|
| Extension/inspection rows waiting on open G/S/F targets | 144 | Only through their targets; they inherit when every target closes. Blockers: G113 (22), S005 (18), G030 (17), G003 (12), G006 (10), G133 (9), G002 (8), G079 (8), G005 (5), G020 (5), G009 (4), G025 (4). |
| Claims worded as IDE surfaces (palette, toolbar, tool window, dialog, gutter, Alt+Enter) | 45 | Part of them (see below). The rest needs the desktop IDE. |
| Not applicable, kept open with a recorded reason | 18 | No. |
| Explicit headless claim not met | 7 | No with the released API (reasons below). |
| No explicit documented claim | 6 | No. Nothing to verify against. |
| Covered but a scenario fails (G107, RS-306) | 1 | Only if the Liquibase decimal syntax is confirmed. |
| No claims file (S005) | 1 | Needs claims; closing it is a judgement call (see findings). |
| Blocked upstream (S007) | 1 | No. Needs a per-document edit guard in `l0ng-ai/openidea`. |

## Findings

1. **The 45 surface rows are not all desktop-only.** Their claims name an IDE element, but about 28 claims in about 22 rows have an OpenIDEA equivalent that a headless scenario can check: an action, a code action, a completion, a pick prompt, a setting. They were marked `verifiableHeadless: false` because the sentence names the original's surface. Examples: G007-C1 and G008-C1 (palette wizards; we have the actions), G010-C1/C2 (validation action), G034-C1/C2 and G035-C1 (callbacks and equals/hashCode actions), G044-C1/C2, G045-C1, G047-C1 (code actions on the caret), G052-C1 (query completion), G115-C1 (preview before saving: our confirm preview), G118-C2 (callback event choice: our pick prompt), G122-C1, G123-C1/C2, G093-C2, G103-C1, G126-C2 (per-DBMS type mappings: `sqlTypeMappings` exists, not per DBMS), G132-C1 (mapTo completion), G133-C4. Closing them would verify the behaviour, not the surface; each scenario would have to say so. That reinterprets the claim, so it needs your decision.
2. **Truly desktop or no API (about 17 rows):** G001, G002, G004 (tool window), G005, G011, G012 (palette, toolbar, panel layout), G030-C2 (settings page), G032-C4, G039, G055-C5/C6, G079 (structure view, gutter navigation), G080-C1, G084-C1, G096, G098 (live IDE data source), G102-C1 (code editor for algorithm naming), G111-C1 (drag and drop), G115-C2 (smart preview), G131-C2, S006.
3. **Seven headless claims that cannot be met:** G018-C1, G025-C1 (palette, toolbar, inspector, structure Add button: host toolbar buttons only dispatch commands), G020-C2 (our rules have their own severities; forcing Warning would downgrade compile-breaking rules), G082-C1 (needs an existing IDE data source), G118-C4 (IDE-side recognition), G122-C2 and G123-C4 (scratch file, database console, clipboard: no plugin API).
4. **S005 and the F rows.** Eighteen inspection rows inherit from S005. Real divergences from the original remain for F002, F003, F015, F018 (Kotlin) and F019 (the original generates equals/hashCode). Closing S005 as "reconciled" would be literally true and would make those rows look at parity when they are not; it is kept open on purpose.
5. **G113 blocks 22 extension rows and has only implied claims.** It cannot close on claims. Whether implied claims may count for a row that only anchors extension rows is an open decision.
6. **Evidence gaps on partial rows.** 23 rows are `partial` with no passing L1 scenario: G001, G002, G004, G006, G007, G008, G010, G023, G034, G044, G047, G052, G070, G081, G087, G102, G107, G122, I008, S001, S002, S003, S006. For G007, G008, G010, G023, G034, G044, G047, G052, G070 and G087 the behaviour exists, so these are the first candidates if finding 1 is accepted.
7. **All closures rest on documentation.** None of the 81 documentation-derived rows has been compared with the original's behaviour, and the 51 inherited rows repeat no test. The decompiled original could upgrade chosen rows to `source-reading`, which no row uses yet.
8. **Unproven behaviour on closed rows.** Several closed rows depend on S007-affected edits across files (rename and sync actions) or on completions whose insert text the runner cannot see (unit-tested only). They are listed in the reports of the agents that built them.

## Row table

| Row | Family | Capability | Status | Impl | L1 | Category | What it needs |
|---|---|---|---|---|---|---|---|
| A001 | action | JpbGenerateEntityAttribute | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G005 |
| A003 | action | JpbSafeDeleteMethodAction | unverified | no | - | not applicable | Runs the IntelliJ platform Safe Delete refactoring (usage search plus conflict dialog) on the PSI element selected in the JPA Buddy tool win |
| A005 | action | JpbCreateRelatedEntityPopup | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G009 |
| A006 | action | JpbCreateIndex | unverified | no | - | extension/inspection row, waits on targets | open targets: G007 |
| A007 | action | JpbGenerateUtilities | unverified | no | - | extension/inspection row, waits on targets | open targets: G034, G035 |
| A008 | action | JpbCreateRepositoryMethod | unverified | no | - | extension/inspection row, waits on targets | open targets: G045 |
| A009 | action | JpbCreateSpringDataSource | unverified | no | - | extension/inspection row, waits on targets | open targets: G081, G082 |
| A010 | action | JpbAddMapperMethods | unverified | no | - | extension/inspection row, waits on targets | open targets: G070 |
| A012 | action | JpbGenerateLifecycleCallback | unverified | no | - | extension/inspection row, waits on targets | open targets: G034 |
| A013 | action | JpbGenerateNamedQuery | unverified | no | - | extension/inspection row, waits on targets | open targets: G008 |
| A014 | action | JpbCreateEntity | unverified | no | - | extension/inspection row, waits on targets | open targets: G013, G003 |
| A015 | action | JpbCreateEntityFromPojo | unverified | no | - | extension/inspection row, waits on targets | open targets: G013 |
| A017 | action | JpbCreateJpaRepository | unverified | no | - | extension/inspection row, waits on targets | open targets: G039 |
| A023 | action | JpbCreateEntityActionWithRelationTo | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G009 |
| A024 | action | JpbCreateEntityActionWithRelationFrom | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G009 |
| A025 | action | JpbCreateJpaConverter | unverified | no | - | extension/inspection row, waits on targets | open targets: G025 |
| A031 | action | JpbReferencesTreeNotification | unverified | no | - | extension/inspection row, waits on targets | open targets: G002 |
| A032 | action | JpbConfigureToolWindow | unverified | no | - | extension/inspection row, waits on targets | open targets: G012 |
| A033 | action | JpbShowJpaSettings | unverified | no | - | extension/inspection row, waits on targets | open targets: G129, G030 |
| A034 | action | JpbShowInspectorPopup | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| A035 | action | JpbRefreshJpaStructure | unverified | no | - | extension/inspection row, waits on targets | open targets: G001 |
| A037 | action | JpbSafeDeleteMethod | unverified | no | - | not applicable | Second registration (different action id) of the same SafeDeleteMethodAction class as A003; identical platform Safe Delete refactoring, whic |
| A038 | action | JpbSafeDeleteClass | unverified | no | - | not applicable | Same platform Safe Delete refactoring as A003, applied to a class (entity or repository) selected in the inspector. No OpenIDEA refactoring  |
| A041 | action | JpbLiquibaseDeleteTag | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| A043 | action | CreateReverseEngineeringItem | unverified | no | - | extension/inspection row, waits on targets | open targets: G086 |
| A044 | action | JpbReEntityAttributeFromDb | unverified | no | - | extension/inspection row, waits on targets | open targets: G096 |
| E001 | Registered editorNotificationProvider | com.intellij.jpb.enp.EntityEditorNotificationProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G011 |
| E003 | Registered codeInsight.lineMarkerProvider | com.intellij.jpb.linemarker.JpaConverterUsesLineMarkerProvid | unverified | no | - | extension/inspection row, waits on targets | open targets: G002, G025 |
| E004 | Registered codeInsight.lineMarkerProvider | com.intellij.jpb.linemarker.JpaConverterUsedByLineMarkerProv | unverified | no | - | extension/inspection row, waits on targets | open targets: G002, G025 |
| E005 | Registered codeInsight.lineMarkerProvider | com.intellij.jpb.linemarker.DtoEntityLineMarkerProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G002, G079 |
| E006 | Registered codeInsight.lineMarkerProvider | com.intellij.jpb.linemarker.DtoEntityAttributeLineMarkerProv | unverified | no | - | extension/inspection row, waits on targets | open targets: G002, G079 |
| E007 | Registered webHelpProvider | com.intellij.jpb.help.JpbHelpProvider | unverified | no | - | not applicable | Web help URL provider; no editor behavior |
| E008 | Registered codeInsight.unresolvedReferenceQuickFixProvider | com.intellij.jpb.codeinsight.CreateSuitableClassQuickFixProv | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E009 | Registered postStartupActivity | com.intellij.jpb.action.persistenceunit.PersistenceUnitStart | unverified | no | - | extension/inspection row, waits on targets | open targets: G004 |
| E010 | Registered postStartupActivity | com.intellij.jpb.exceptions.handling.ExceptionEventHandler | unverified | no | - | not applicable | Plugin error-report event handler; IDE telemetry only |
| E011 | Registered implicitUsageProvider | com.intellij.jpb.usages.LifecycleCallbackImplicitUsageProvid | unverified | no | - | extension/inspection row, waits on targets | open targets: G034 |
| E013 | Registered implicitUsageProvider | com.intellij.jpb.usages.MigrationAndCallbackImplicitUsagePro | unverified | no | - | extension/inspection row, waits on targets | open targets: G118 |
| E014 | Registered referencesSearch | com.intellij.jpb.reference.search.MappedByUsagesSearcher | unverified | no | - | extension/inspection row, waits on targets | open targets: G079 |
| E015 | Registered methodReferencesSearch | com.intellij.jpb.reference.search.MapstructUsagesSearcher | unverified | no | - | extension/inspection row, waits on targets | open targets: G132 |
| E016 | Registered psi.referenceContributor | com.intellij.jpb.reference.JpabUastReferenceContributor | unverified | no | - | extension/inspection row, waits on targets | open targets: G079 |
| E018 | Registered annotator | com.intellij.jpb.reference.hard.JpabLiteralExpressionAnnotat | unverified | no | - | extension/inspection row, waits on targets | open targets: G009, G070 |
| E019 | Registered lang.inspectionSuppressor | com.intellij.jpb.inspection.EntityInspectionSuppressor | unverified | no | - | extension/inspection row, waits on targets | open targets: G030 |
| E021 | Registered completion.contributor | repositoryMethodHandle | unverified | no | - | extension/inspection row, waits on targets | open targets: G052, G131 |
| E022 | Registered completion.contributor | fieldCompletionContributor | unverified | no | - | extension/inspection row, waits on targets | open targets: G052 |
| E023 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.DtoDeclarationConfigurab | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E024 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.ProjectionDeclarationCon | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E025 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.RepositoryConfigurableOp | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E026 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.MapperDeclarationConfigu | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E027 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.JpaIoCConfigurableOption | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E028 | Registered search.optionContributor | com.intellij.jpb.ui.settings.search.JpaDesignersConfigurable | unverified | no | - | not applicable | Settings-search option contributor; IntelliJ settings search indexing only |
| E029 | Registered daemon.highlightInfoFilter | com.intellij.jpb.inspection.JpaHighlightErrorFilter | unverified | no | - | extension/inspection row, waits on targets | open targets: G030 |
| E032 | Registered orm.ormUnitsProvider | com.intellij.jpb.orm.JpabOrmUnitsProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G004 |
| E033 | Registered lang.logicalStructureElementsProvider | com.intellij.jpb.structuretw.OrmEntityDtosProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G001, G002 |
| E034 | Registered presentationProvider | com.intellij.jpb.structuretw.EntityDtoModelPresentationProvi | unverified | no | - | extension/inspection row, waits on targets | open targets: G001 |
| E035 | Registered presentationProvider | com.intellij.jpb.structuretw.OrmEntityDtosProviderPresentati | unverified | no | - | extension/inspection row, waits on targets | open targets: G001 |
| E036 | Registered ide.dynamicPluginVetoer | com.intellij.jpb.listener.JpaDynamicPluginVetoer | unverified | no | - | not applicable | Dynamic plugin load/unload veto; IntelliJ plugin-lifecycle infrastructure |
| E037 | Registered springDataSourceService | com.intellij.jpb.action.creation.ds.PropertiesSpringDataSour | unverified | no | - | extension/inspection row, waits on targets | open targets: G081 |
| E038 | Registered eventDataProvider | com.intellij.jpb.service.JpabEventDataProvider | unverified | no | - | not applicable | Platform plumbing: supplies the action-event machinery with two facts, that the tool window toolbar place counts as a global action place an |
| E040 | Registered inspectorActionsProvider | com.intellij.jpb.ui.toolwindow.inspector.JpaInspectorActions | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E041 | Registered entityDtoSearcher | com.intellij.jpb.action.creation.entity.MapstructDtoSearcher | unverified | no | - | extension/inspection row, waits on targets | open targets: G079 |
| E042 | Registered entityDtoSearcher | com.intellij.jpb.action.creation.entity.CommentDtoSearcher | unverified | no | - | extension/inspection row, waits on targets | open targets: G079 |
| E043 | Registered entityDtoSearcher | com.intellij.jpb.action.creation.entity.NamePatternDtoSearch | unverified | no | - | extension/inspection row, waits on targets | open targets: G079 |
| E044 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.inspector.item.loader.EntityA | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E045 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.inspector.item.loader.EntityA | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E046 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.inspector.item.loader.EntityI | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E047 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.inspector.RepositoryItemLoade | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E048 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.inspector.RepositoryMethodIte | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E049 | Registered inspectorItemLoader | com.intellij.jpb.ui.toolwindow.palette.item.nq.EntityNamedQu | unverified | no | - | extension/inspection row, waits on targets | open targets: G006, G008 |
| E052 | Registered dtoFieldSearcherEp | com.intellij.jpb.action.creation.entity.MapStructDtoFieldSea | unverified | no | - | extension/inspection row, waits on targets | open targets: G070 |
| E053 | Registered errorMessageMutatorEP | com.intellij.jpb.logger.IdeaErrorMessageMutator | unverified | no | - | not applicable | Logger error-message mutator; IntelliJ error reporting only |
| E054 | Registered errorMessageSuppressorEP | com.intellij.jpb.logger.IdeaErrorMessageSuppressor | unverified | no | - | not applicable | Logger error-message suppressor; IntelliJ error reporting only |
| E055 | Registered errorMessageSuppressorEP | com.intellij.jpb.logger.ControlFlowExceptionSuppressor | unverified | no | - | not applicable | Logger control-flow exception suppressor; IntelliJ error reporting only |
| E059 | Registered expressionCompletionContributor | com.intellij.jpb.completion.bean.injection.EntityExpressionC | unverified | no | - | extension/inspection row, waits on targets | open targets: G131 |
| E060 | Registered expressionCompletionContributor | com.intellij.jpb.completion.bean.injection.MapToExpressionCo | unverified | no | - | extension/inspection row, waits on targets | open targets: G132 |
| E061 | Registered expressionCompletionContributor | com.intellij.jpb.completion.bean.injection.ModelMapperExpres | unverified | no | - | extension/inspection row, waits on targets | open targets: G073, G132 |
| E062 | Registered expressionCompletionContributor | com.intellij.jpb.completion.bean.injection.SpringRepositoryE | unverified | no | - | extension/inspection row, waits on targets | open targets: G131 |
| E063 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.UnresolvedRepositoryReferenceQu | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E064 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.ProjectionUnresolvedQuickFixPro | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E065 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.JpaDefaultQuickFixProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E066 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.RepoReturnTypeUnresolvedDtoRefQ | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E067 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.ModelMapperUnresolvedDtoRefQuic | unverified | no | - | extension/inspection row, waits on targets | open targets: G133, G073 |
| E068 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.MapStructUnresolvedDtoRefQuickF | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E069 | Registered unresolvedClassReferenceQuickFixProvider | com.intellij.jpb.codeinsight.UnresolvedDtoRefQuickFixProvide | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E071 | Registered paletteContributor | com.intellij.jpb.ui.toolwindow.palette.type.EntityPaletteTyp | unverified | no | - | extension/inspection row, waits on targets | open targets: G005 |
| E072 | Registered paletteContributor | com.intellij.jpb.ui.toolwindow.palette.type.RepositoryPalett | unverified | no | - | extension/inspection row, waits on targets | open targets: G005 |
| E073 | Registered paletteContributor | com.intellij.jpb.ui.toolwindow.palette.type.EntityFilePalett | unverified | no | - | extension/inspection row, waits on targets | open targets: G005 |
| E074 | Registered paletteContributor | com.intellij.jpb.ui.toolwindow.palette.type.RepositoryFilePa | unverified | no | - | extension/inspection row, waits on targets | open targets: G005 |
| E077 | Registered group | com.intellij.jpa.jpb.model.GenerateGroup | unverified | no | - | extension/inspection row, waits on targets | open targets: G003 |
| E078 | Registered group | com.intellij.jpb.structure.new.own | unverified | no | - | extension/inspection row, waits on targets | open targets: G003 |
| E079 | Registered group | com.intellij.jpb.structure.new | unverified | no | - | extension/inspection row, waits on targets | open targets: G003 |
| E080 | Registered group | com.intellij.jpb.structure.toolbar.new | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G011 |
| E081 | Registered group | com.intellij.jpb.structure.toolbar | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G011 |
| E082 | Registered group | com.intellij.jpb.creationGroupProjectTree | unverified | no | - | extension/inspection row, waits on targets | open targets: G003, G013 |
| E083 | Registered group | com.intellij.jpb.group.otherNewGroup | unverified | no | - | extension/inspection row, waits on targets | open targets: G003 |
| E085 | Registered group | com.intellij.jpb.ui.toolwindow.inspector.JpaInspectorActions | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E086 | Registered group | com.intellij.jpb.ui.toolwindow.inspector.JpaInspectorActions | unverified | no | - | extension/inspection row, waits on targets | open targets: G006 |
| E087 | Registered errorMessageMutatorEP | com.intellij.jpb.kt.logger.IdeaResolutionExceptionMutator | unverified | no | - | not applicable | Kotlin logger error-message mutator; IntelliJ error reporting only |
| E088 | Registered errorMessageSuppressorEP | com.intellij.jpb.kt.logger.KotlinErrorMessageSuppressor | unverified | no | - | not applicable | Kotlin logger error-message suppressor; IntelliJ error reporting only |
| E090 | Registered codeInsight.unresolvedReferenceQuickFixProvider | com.intellij.jpb.kt.codeinsight.KotlinQuickFixProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G133 |
| E091 | Registered codeInsight.lineMarkerProvider | com.intellij.jpb.kt.model.dto.KtDtoEntityAttributeLineMarker | unverified | no | - | extension/inspection row, waits on targets | open targets: G079, G002 |
| E093 | Registered annotator | com.intellij.jpa.jpb.model.inspection.AttributeTypeAnnotator | unverified | no | - | extension/inspection row, waits on targets | open targets: F002 |
| E094 | Registered lang.inspectionSuppressor | com.intellij.jpb.database.suppressor.JpaInspectionSuppressor | unverified | no | - | extension/inspection row, waits on targets | open targets: G030 |
| E095 | Registered javaee.jpa.entityLineMarkerActionsProvider | com.intellij.jpb.database.jpa.JpabLinemarkerActionsProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G002 |
| E096 | Registered languageInjectionContributor | com.intellij.jpb.database.jpa.JpqlInjector | unverified | no | - | extension/inspection row, waits on targets | open targets: G052 |
| E097 | Registered editorNotificationProvider | com.intellij.jpb.liquibase.enp.LiquibaseEditorNotificationPr | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E099 | Registered inspectorActionsProvider | com.intellij.jpb.liquibase.toolwindow.inspector.JpaDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E100 | Registered inspectorItemLoader | com.intellij.jpb.liquibase.toolwindow.inspector.item.loader. | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E101 | Registered inspectorModelResolver | com.intellij.jpb.liquibase.toolwindow.inspector.LiquibaseIns | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E103 | Registered paletteContributor | com.intellij.jpb.liquibase.toolwindow.palette.type.Liquibase | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E104 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E105 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E106 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E107 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E108 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E109 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E110 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E111 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E112 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E113 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E114 | Registered inspectorItemProvider | com.intellij.jpb.liquibase.toolwindow.inspector.prop.descrip | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E118 | Registered group | com.intellij.jpb.liquibase.toolwindow.inspector.JpaDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E119 | Registered group | com.intellij.jpb.liquibase.toolwindow.inspector.JpaDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E120 | Registered editorNotificationProvider | com.intellij.jpb.flyway.enp.SqlEditorNotificationProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G115 |
| E121 | Registered entityEditorAction.provider | com.intellij.jpb.flyway.toolwindow.DdlEntityEditorNotificati | unverified | no | - | extension/inspection row, waits on targets | open targets: G122 |
| E123 | Registered inspectorActionsProvider | com.intellij.jpb.flyway.toolwindow.inspector.FlywayDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E128 | Registered group | com.intellij.jpb.flyway.toolwindow.inspector.FlywayDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E129 | Registered group | com.intellij.jpb.flyway.toolwindow.inspector.FlywayDdlInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: G113 |
| E132 | Registered group | com.intellij.jpb.flyway.database.console | unverified | no | - | extension/inspection row, waits on targets | open targets: G123 |
| E134 | Registered entityEditorAction.provider | com.intellij.jpb.re.enp.ReEntityEditorNotificationProvider | unverified | no | - | extension/inspection row, waits on targets | open targets: G096 |
| E135 | Registered inspectorActionsProvider | com.intellij.jpb.re.toolwindow.inspector.JpaReInspectorActio | unverified | no | - | extension/inspection row, waits on targets | open targets: G096 |
| E136 | Registered palette.entityItems | com.intellij.jpb.re.toolwindow.palette.ReEntityPaletteItemsC | unverified | no | - | extension/inspection row, waits on targets | open targets: G087 |
| E138 | Registered group | com.intellij.jpa.re.jpbCreationGroupProjectTree | unverified | no | - | extension/inspection row, waits on targets | open targets: G086 |
| F001 | Inspection families | Column mapping on a relationship | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F002 | Inspection families | Unsupported persistent field type | unverified | no | - | extension/inspection row, waits on targets | open targets: S005, G030 |
| F003 | Inspection families | Serializable composite key | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F004 | Inspection families | Converter type compatibility | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F006 | Inspection families | Entity-valued field without relationship mapping | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F007 | Inspection families | Absent identifier | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F008 | Inspection families | Conflicting identity declarations | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F009 | Inspection families | Equality owner/class checks | unverified | no | - | extension/inspection row, waits on targets | open targets: S005, G020, G035 |
| F010 | Inspection families | Converter role annotation | unverified | no | - | extension/inspection row, waits on targets | open targets: S005, G025 |
| F011 | Inspection families | Multiple version fields | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F012 | Inspection families | Paged results without paging input | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F013 | Inspection families | Inverse one-to-one lazy caveat | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F014 | Inspection families | Table mapping under single-table inheritance | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F015 | Inspection families | Kotlin generated equality/string methods | unverified | no | - | extension/inspection row, waits on targets | open targets: S005, G020 |
| F016 | Inspection families | Lombok string loading/recursion | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F017 | Inspection families | Write query marker | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F018 | Inspection families | Lombok constructor generation | unverified | no | - | extension/inspection row, waits on targets | open targets: S005 |
| F019 | Inspection families | Lombok entity equality | unverified | no | - | extension/inspection row, waits on targets | open targets: S005, G020 |
| G001 | Entity Designer | Structure browsing | partial | yes | - | surface (desktop-worded) claims | G001-C1; G001-C2 |
| G002 | Entity Designer | Related artifacts | partial | yes | - | surface (desktop-worded) claims | G002-C1; G002-C2; G002-C3 |
| G003 | Entity Designer | Structure creation | unverified | no | - | surface (desktop-worded) claims | G003-C1; G003-C2 |
| G004 | Entity Designer | Persistence units | partial | yes | - | surface (desktop-worded) claims | G004-C1; G004-C2 |
| G005 | Entity Designer | Context palette | unverified | no | - | surface (desktop-worded) claims | G005-C1; G005-C2; G005-C3 |
| G006 | Entity Designer | Mapping inspector | partial | yes | - | surface (desktop-worded) claims | G006-C1 |
| G007 | Entity Designer | Index wizard | partial | yes | - | surface (desktop-worded) claims | G007-C1 |
| G008 | Entity Designer | Named-query wizard | partial | yes | - | surface (desktop-worded) claims | G008-C1 |
| G009 | Entity Designer | Association advice | partial | yes | pass | surface (desktop-worded) claims | G009-C1 |
| G010 | Entity Designer | Validation editor | partial | yes | - | surface (desktop-worded) claims | G010-C1; G010-C2 |
| G011 | Entity Designer | Editor toolbar | unverified | no | - | surface (desktop-worded) claims | G011-C1; G011-C2; G011-C3 |
| G012 | Entity Designer | Panel layout | unverified | no | - | surface (desktop-worded) claims | G012-C1; G012-C2; G012-C3; G012-C4 |
| G013 | Entity Designer | Entity generation | partial | yes | pass | surface (desktop-worded) claims | G013-C1 |
| G018 | Entity Designer | Add identifier | partial | yes | pass | headless claim not met | G018-C1 |
| G020 | Entity Designer | Lombok equality checks | partial | yes | pass | headless claim not met | G020-C2 |
| G023 | Entity Designer | Lombok annotation editor | partial | yes | - | surface (desktop-worded) claims | G023-C1 |
| G025 | Entity Designer | Converter creation | partial | yes | pass | headless claim not met | G025-C1 |
| G030 | Entity Designer | Inspection configuration | partial | yes | pass | surface (desktop-worded) claims | G030-C2 |
| G032 | Entity Designer | Physical naming | partial | yes | pass | surface (desktop-worded) claims | G032-C4 |
| G034 | Entity Designer | Lifecycle templates | partial | yes | - | surface (desktop-worded) claims | G034-C1; G034-C2 |
| G035 | Entity Designer | Equality utilities | partial | yes | pass | surface (desktop-worded) claims | G035-C1 |
| G039 | Spring Data | Repository grouping | unverified | no | - | surface (desktop-worded) claims | G039-C1; G039-C2 |
| G044 | Spring Data | Attribute query intention | partial | yes | - | surface (desktop-worded) claims | G044-C1; G044-C2 |
| G045 | Spring Data | Unresolved method creation | unverified | no | - | surface (desktop-worded) claims | G045-C1 |
| G047 | Spring Data | Existing query editing | partial | yes | - | surface (desktop-worded) claims | G047-C1 |
| G052 | Spring Data | Query completion | partial | yes | - | surface (desktop-worded) claims | G052-C1 |
| G055 | Spring Data | Projection conventions | unverified | no | pass | surface (desktop-worded) claims | G055-C5; G055-C6 |
| G070 | DTOs and mappers | Mapping methods | partial | yes | - | surface (desktop-worded) claims | G070-C1 |
| G073 | DTOs and mappers | ModelMapper | partial | yes | pass | surface (desktop-worded) claims | G073-C2 |
| G079 | DTOs and mappers | Related navigation | unverified | no | - | surface (desktop-worded) claims | G079-C1; G079-C2 |
| G080 | Database connections | Connection management | unverified | no | - | surface (desktop-worded) claims | G080-C1; G080-C2 |
| G081 | Database connections | Datasource discovery | partial | yes | - | surface (desktop-worded) claims | G081-C1 |
| G082 | Database connections | Datasource generation | partial | yes | pass | headless claim not met | G082-C1 |
| G084 | Database connections | Schema selection | unverified | no | pass | surface (desktop-worded) claims | G084-C1 |
| G086 | Reverse engineering | Database selection | unverified | no | - | no explicit claim | nothing to verify against |
| G087 | Reverse engineering | Mapped-object inventory | partial | yes | - | surface (desktop-worded) claims | G087-C1; G087-C2 |
| G093 | Reverse engineering | Unknown SQL types | partial | yes | pass | surface (desktop-worded) claims | G093-C2 |
| G096 | Reverse engineering | Existing entity updates | unverified | no | - | surface (desktop-worded) claims | G096-C1; G096-C2 |
| G097 | Reverse engineering | Cardinality inference | partial | yes | pass | surface (desktop-worded) claims | G097-C5 |
| G098 | Reverse engineering | Schema cache | unverified | no | - | surface (desktop-worded) claims | G098-C1 |
| G102 | Reverse engineering | Naming algorithms | partial | yes | - | surface (desktop-worded) claims | G102-C1 |
| G103 | Reverse engineering | Vendor mappings | partial | yes | pass | surface (desktop-worded) claims | G103-C1 |
| G107 | Database versioning | Initialization | partial | yes | fail | covered, scenario failing | RS-306 decimal syntax |
| G111 | Database versioning | Change grouping | partial | yes | pass | surface (desktop-worded) claims | G111-C1; G111-C3; G111-C4 |
| G113 | Database versioning | Changelog inspector | unverified | no | - | no explicit claim | nothing to verify against |
| G115 | Database versioning | Update/SQL preview | partial | yes | pass | surface (desktop-worded) claims | G115-C1; G115-C2 |
| G118 | Database versioning | SQL callbacks | partial | yes | pass | headless claim not met | G118-C4 |
| G122 | Database versioning | Single-entity DDL | partial | yes | - | headless claim not met | G122-C2 |
| G123 | Database versioning | SQL designer | unverified | no | pass | headless claim not met | G123-C4 |
| G126 | Database versioning | Custom SQL types | partial | yes | pass | surface (desktop-worded) claims | G126-C2 |
| G127 | Database versioning | Naming strategies | partial | yes | pass | surface (desktop-worded) claims | G127-C3 |
| G129 | Database versioning | Shared settings | unverified | no | - | no explicit claim | nothing to verify against |
| G131 | Coding assistance and minimalistic access | Method calls | unverified | no | pass | surface (desktop-worded) claims | G131-C2 |
| G132 | Coding assistance and minimalistic access | Mapper calls | partial | yes | pass | surface (desktop-worded) claims | G132-C1 |
| G133 | Coding assistance and minimalistic access | Unresolved objects | unverified | no | pass | surface (desktop-worded) claims | G133-C4 |
| I001 | localInspection | com.intellij.jpb.inspection.EntityIdMissingInspection | unverified | no | - | extension/inspection row, waits on targets | open targets: F007, G030 |
| I002 | localInspection | com.intellij.jpb.inspection.DuplicateVersionAnnotationInspec | unverified | no | - | extension/inspection row, waits on targets | open targets: F011, G030 |
| I003 | localInspection | com.intellij.jpb.inspection.UnsupportedTypeWithoutConverterI | unverified | no | - | extension/inspection row, waits on targets | open targets: F002, G030 |
| I004 | localInspection | com.intellij.jpb.inspection.LombokToStringLazyInspection | unverified | no | - | extension/inspection row, waits on targets | open targets: F016, G030 |
| I005 | localInspection | com.intellij.jpb.inspection.LombokToStringLazyIncludeInspect | unverified | no | - | extension/inspection row, waits on targets | open targets: F016, G030 |
| I006 | localInspection | com.intellij.jpb.inspection.LombokEqualsAndHashCodeInspectio | unverified | no | - | extension/inspection row, waits on targets | open targets: F019, G020, G030 |
| I007 | localInspection | com.intellij.jpb.inspection.LombokDataInspection | unverified | no | - | extension/inspection row, waits on targets | open targets: F019, G020, G030 |
| I008 | localInspection | com.intellij.jpb.inspection.NoArgsConstructorInspection | partial | yes | - | extension/inspection row, waits on targets | open targets: F018, G030 |
| I009 | localInspection | com.intellij.jpb.inspection.ConverterSpecifiedIncorrectlyIns | unverified | no | - | extension/inspection row, waits on targets | open targets: F010, F004, G030 |
| I010 | localInspection | com.intellij.jpb.inspection.IncompatibleTableAnnotationInspe | unverified | no | - | extension/inspection row, waits on targets | open targets: F014, G030 |
| I011 | localInspection | com.intellij.jpb.inspection.CompositeIdClassSerializableInsp | unverified | no | - | extension/inspection row, waits on targets | open targets: F003, G030 |
| I012 | localInspection | com.intellij.jpb.kt.model.inspection.DataClassEqualsAndHashC | unverified | no | - | extension/inspection row, waits on targets | open targets: F015, G030 |
| S001 | Designer controls | Conditional precision/scale, orphan removal, collection orde | partial | yes | - | no explicit claim | nothing to verify against |
| S002 | Designer controls | Join/collection tables, overrides and ownership-aware contro | partial | yes | - | no explicit claim | nothing to verify against |
| S003 | Designer controls | Inheritance strategy, discriminator and primary-key join con | partial | yes | - | no explicit claim | nothing to verify against |
| S005 | Inspection families | All 19 screenshot-derived rule families reconciled against c | unverified | no | - | no claims file | extract claims from public docs / original |
| S006 | Interaction | Selection synchronization, pinning, draft retention and keyb | partial | no | - | surface (desktop-worded) claims | S006-C1 |
| S007 | Semantics | Atomic and version-checked application across every modified | blocked | no | - | blocked upstream (S007) | Released FileAction edits do not expose an expected version for each existing target document. Preview/export cannot close application parit |
