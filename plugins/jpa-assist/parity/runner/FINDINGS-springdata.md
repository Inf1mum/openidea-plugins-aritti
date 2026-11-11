# Spring Data / repository / query / projection / DTO / mapper family: scenario findings

L1 documentation-derived evidence only (runner README). No parity claim, no ledger row edited. Scenarios RS-100..RS-133 (34 files),
expectations taken from the claim text, never from our output. Every scenario was run 3 times, one runner process at a time under
`/tmp/jpa-runner.lock`; all 34 validated (`--validate`, 0 errors). Receipts were byte-identical across the three runs of every scenario.
Earlier failures seen under machine load (host drops provider results over 50 ms) were discarded; only the 3-run results below count.

## Results per scenario (3 runs)

Pass/pass/pass: RS-100..RS-117, RS-119, RS-121..RS-132 (30 scenarios).
Fail/fail/fail (findings): RS-118, RS-120, RS-124, RS-133.

## Findings (our behaviour does not meet the documented claim)

| Scenario | Claim | Documented | Observed (3/3 runs) |
|---|---|---|---|
| RS-118 | G062-C1 | Lombok immutable DTO: `@Value` with redundant access modifiers removed | `@Value` is emitted but the fields are written `private final Long id; private final String name;` (and `public class`), so the redundant modifiers remain |
| RS-120 | G067-C3 / G068-C1 | DTO generation is not limited to JPA entities | "Create scalar DTO with preview..." is not offered on a plain non-entity class (offered list: entity generation actions and "Convert class to record..." only) |
| RS-124 | G078-C4 | Validation transfer requires Hibernate Validator or the validation starter | With `spring-boot-starter-validation` removed from the pom and `@NotBlank` on the entity, the "Bean Validation transfer" prompt still offers `copy`; the action never reads the build for it |
| RS-133 | G066-C1 | Record form appears only if the project SDK is 16 or higher | On the Java 11 legacy fixture the record scalar DTO action is offered and runs to its name prompt |

## Per row

| Row | Covered (scenario) | Failing | Not expressible / not offered | Conclusion |
|---|---|---|---|---|
| G036 | C1 RS-100 (source root/package not offered), C3 RS-101 (+RS-002) | none | C2 any project repository as parent: parent list is fixed to Spring Data interfaces | not all explicit claims covered (C2 not offered) |
| G040 | C1,C2,C4 RS-001; C3 RS-102 (alias not offered) | none | alias on subject keyword; named/positional parameters (C1 sentence part) | partial |
| G053 | C1,C3 RS-109; C2 RS-110 | none | source root/package; projection type per association (nested closed interfaces only) | covered at option level, passing |
| G056 | C1 RS-111 | none | C2 and C3 (warn when @EnableJpaAuditing missing / AuditingEntityListener not registered): no such inspection exists | not covered (C2, C3 not offered) |
| G057 | C2,C3 RS-129 | none | C1 selecting Entity Views inside the standard repository window: separate action instead | partial |
| G058 | C1,C2,C3 RS-130 | none | the creation-window checkbox does not exist (separate action) | all explicit claims covered at option level, passing |
| G059 | C1 RS-005; C2 RS-114 (no Mutable toggle, record only); C3,C4,C5 RS-112 | none | Mutable toggle, final fields of a class | covered at option level, passing |
| G062 | C2 RS-119; C3 RS-117 | C1 RS-118 | none | not closed: C1 fails |
| G067 | none passing | C3 RS-120 | C1 "Entity from POJO" action and C2 cardinality detection: no such action | not covered |
| G069 | C2 RS-125; C1 RS-126 (dependency gate only; DTO-window mapper field not offered) | none | C3 naming strategy for collection mappings not offered | partial |
| G073 | C1,C3,C4 RS-127 | none | C2 postfix completion (not headless, claim not verifiableHeadless) | explicit headless claims covered, passing |
| G074 | C1,C3 RS-128 (parent view, package/source root not offered) | none | C2 creatable/updatable, C4 setters for all attributes, C5 inner entity views, C6 view in standard repository window (RS-129 is a separate action) | partial |
| G078 | C2 RS-123 | C4 RS-124 | C1 per-field constraint configuration in the wizard, C3 enable/disable each constraint and edit message: only copy/none is offered | not closed: C4 fails |
| G132 | none | none | C2,C3 "mapTo..." postfix scaffolding of a mapping call: only "Inject mapper..." (inject, no call) exists; C1 not headless | not covered |
| G002 | none | none | no headless claim (all 3 explicit claims are not verifiableHeadless) | n/a |

Other rows of the area:

| Row | Covered | Failing | Not expressible / not offered |
|---|---|---|---|
| G041 | C1 RS-103 | none | C2 inline "+" creation of projections/DTOs |
| G042 / S004 | G042-C2, S004-C1, S004-C4 RS-104 (G042-C1 in RS-001) | none | S004-C2 and G043-C1 named vs positional parameters and an independent Pageable switch (Pageable only follows the Page wrapper); S004-C3 inline "+" |
| G046 | none | none | approved-keyword restriction setting does not exist |
| G048 | C1 RS-105 (wizard layout not checked) | none | |
| G049 | C1 RS-106 | none | only CompletableFuture is offered (Future, ListenableFuture are not) |
| G050 | C1 RS-107 | none | |
| G051 | C1 RS-108 | none | |
| G060 | C1,C2 RS-113 | none | "only with Mutable" toggle |
| G061 | C1 RS-115, C2 RS-116 | none | option is listed then refused, not hidden |
| G063 / G065 | G063-C2, G065-C1 RS-121 | none | G063-C1 four wizard forms (New Class / New Nested Class / Existing / Flat) do not map one to one |
| G064 | C1 RS-122 | none | C2 comment-link and class-name regexps not offered |
| G066 | none passing | C1 RS-133 | C3 not attempted |
| G068 | C2 RS-114 | C1 RS-120 | |
| G071, G072 | not attempted (G071 base-mapper extension, G072 naming conventions: only the mapper name input and componentModel exist) | | |
| G130 | C1 RS-131 (typing not offered), C3 RS-132 | none | |
| G134 | C2 RS-132 | none | C1 automatic detection of the injection type not offered |
| G037, G038 | already RS-002 | | |

No row is declared closed or at parity by this file.

## Commands

```
export PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=/tmp/jpa-shared-target
(cd plugins/jpa-assist && /tmp/openidea-cli-0142/bin/openidea-plugin build)
cargo build --manifest-path scripts/runtime-smoke/Cargo.toml --bin jpa_assist
BIN=$CARGO_TARGET_DIR/debug/jpa_assist
$BIN "$PWD" --validate --only RS-100,...,RS-133
mkdir /tmp/jpa-runner.lock   # retry until it succeeds
$BIN "$PWD" OUT FIXTURE --scenario plugins/jpa-assist/parity/runner/scenarios --only RS-1xx
rmdir /tmp/jpa-runner.lock   # per scenario, 3 runs each, receipts compared by sha256
```
