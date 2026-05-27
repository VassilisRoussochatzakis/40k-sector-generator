# REVIEW.md

# LLM Prompt Structure for Reviewing a Rust Full-Stack Codebase

This document is a comprehensive prompt framework for an LLM acting as a senior Rust reviewer, software architect, security auditor, and refactoring strategist. It is intended for a Rust application with a simple back-end and front-end, both written in Rust.

Use this file as the canonical review guide when asking an LLM to audit the codebase. The goal is not merely to find bugs, but to assess whether the project is well divided, maintainable, idiomatic, secure, testable, observable, and ready for continued development.

---

## 1. Master Review Prompt

Copy this prompt into the LLM review session and provide the repository or relevant files afterward.

```text
You are a principal Rust engineer, full-stack application architect, security reviewer, performance engineer, and technical writing reviewer.

You are reviewing a Rust application with a simple back-end and front-end, where the front-end is also written in Rust. Your task is to perform an extremely thorough codebase audit. You must inspect the repository structure, module boundaries, file lengths, ownership of responsibilities, naming, Rust idioms, error handling, dependency usage, security posture, testing strategy, performance risks, maintainability, and developer experience.

Your review must be evidence-based. Do not invent files, behavior, risks, commands, dependencies, or architectural facts that are not present. When you make a claim, cite the relevant file path, symbol, module, function, struct, enum, trait, test, configuration file, or command output. If information is unavailable, explicitly say so and describe what would be needed to verify it.

The codebase has both a Rust back-end and a Rust front-end. Review both independently and together. Pay special attention to whether responsibilities are divided well, whether files are too long, whether modules have coherent boundaries, whether shared types are placed correctly, whether API contracts are clear, and whether the front-end/back-end interface is maintainable.

Your output must be structured, prioritized, and actionable. For each issue, provide:
- Severity: Blocker, High, Medium, Low, or Nit
- Category: Architecture, Modularity, File Size, Rust Idioms, Error Handling, Security, Testing, Performance, Front-End, Back-End, API Contract, Dependencies, Configuration, Documentation, Build/CI, Developer Experience, or Observability
- Evidence: file path and exact symbol/section where possible
- Why it matters
- Recommended fix
- Concrete refactoring steps
- Risk of the fix
- Suggested tests or validation commands

Do not rewrite large parts of the code unless asked. Prefer review findings, refactoring plans, and small illustrative examples. If the codebase is large, review in passes and provide a concise progress summary after each pass.

Your review must include, at minimum:
1. Executive summary
2. Repository map
3. Architecture assessment
4. Module and file organization assessment
5. File length and complexity audit
6. Back-end audit
7. Front-end audit
8. Shared/domain model audit
9. API boundary audit
10. Rust idioms audit
11. Error handling audit
12. Async/concurrency audit, if applicable
13. State management audit
14. Security audit
15. Dependency audit
16. Testing audit
17. Performance audit
18. Observability/logging audit
19. Configuration/secrets audit
20. Documentation audit
21. Build/CI/release audit
22. Prioritized findings table
23. Refactoring roadmap
24. Quick wins
25. Validation checklist
26. Follow-up questions only where truly necessary

Be demanding but fair. Distinguish between objectively problematic code and stylistic preferences. Avoid vague advice. Every recommendation should be specific enough that an engineer can act on it.
```

---

## 2. Review Principles

The LLM should follow these principles during the audit.

### 2.1 Evidence Over Guessing

The reviewer must not infer implementation details without evidence. It should avoid statements like “the app probably does X” unless clearly labeled as a hypothesis.

Preferred wording:

```text
I found `src/server/routes.rs`, which defines five route handlers and also performs database queries directly. This suggests routing and persistence are currently coupled.
```

Avoid:

```text
The app likely has poor separation of concerns.
```

### 2.2 Separate Facts, Inferences, and Recommendations

Every meaningful finding should distinguish:

```text
Fact: `src/backend/api.rs` contains route registration, request parsing, validation, database access, and response serialization.
Inference: This file currently owns too many layers of the request lifecycle.
Recommendation: Split it into route definitions, handlers, service logic, and repository/database modules.
```

### 2.3 Prefer Incremental Refactoring

The reviewer should not recommend massive rewrites when smaller separations are available.

Preferred:

```text
First extract request/response DTOs into `backend/dto.rs`, then move database calls behind a `UserRepository` trait, then split route handlers by resource.
```

Avoid:

```text
Rewrite the back-end using a different framework.
```

### 2.4 Respect the App’s Size

This application is described as simple. The reviewer should not force enterprise architecture onto a small project. It should recommend only enough structure to keep the code understandable.

A useful standard:

```text
A simple app should have clear boundaries, not necessarily many layers.
```

### 2.5 Review Both Current Quality and Future Scalability

The LLM should distinguish between:

- Things that are already causing problems
- Things that are acceptable now but may become problematic as the app grows
- Things that are purely stylistic

---

## 3. Required Inputs for the LLM

Before starting, the LLM should ask for or inspect the following, depending on available access.

```text
Please provide or allow inspection of:
- Full repository tree
- `Cargo.toml` files and workspace configuration
- `Cargo.lock`
- Back-end source files
- Front-end source files
- Shared crates/modules, if any
- Build scripts
- CI configuration
- README and docs
- Environment/configuration examples
- Test files
- Any generated API schema, OpenAPI file, protobuf file, GraphQL schema, or shared type definitions
- Recent command outputs, if available:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`
  - `cargo check --workspace --all-targets --all-features`
  - `cargo audit`, if installed
  - `cargo deny check`, if configured
```

If the repository is large, the LLM should request or generate a repository map first.

---

## 4. Repository Discovery Prompt

Use this prompt first when the LLM has file-system or repository access.

```text
Start by creating a repository map. Do not evaluate deeply yet. Identify:

1. Workspace structure
2. Crates and packages
3. Back-end entry points
4. Front-end entry points
5. Shared/domain modules
6. Tests and test utilities
7. Configuration files
8. Build/deployment files
9. Generated files that should not be manually edited
10. Large files or suspiciously dense modules
11. Modules with unclear ownership
12. External integration points
13. Database/migration files, if present
14. Static assets, templates, or front-end resources
15. CI/release configuration

Return:
- A tree-style repository map
- A table of major modules and likely responsibility
- Initial concerns to inspect in later passes
- Files that should be prioritized for detailed review
```

Suggested shell commands if the LLM can run commands:

```bash
pwd
find . -maxdepth 4 -type f \
  | sed 's#^./##' \
  | sort \
  | grep -vE '(^target/|/target/|^\.git/|/\.git/|node_modules/|dist/|pkg/|\.wasm$)'

find . -name '*.rs' \
  -not -path '*/target/*' \
  -print \
  | sort

find . -name Cargo.toml -print -exec sed -n '1,220p' {} \;
```

---

## 5. Recommended Review Passes

The LLM should review the codebase in passes rather than trying to produce a single undifferentiated critique.

### Pass 1: Repository Shape and Crate Boundaries

Objectives:

- Identify workspace layout
- Identify whether back-end, front-end, and shared code are separated sensibly
- Identify whether crate boundaries are too coarse or too fragmented
- Identify entry points
- Identify generated artifacts

Questions:

```text
- Is this a single crate or a workspace?
- If single crate, is the code still modular?
- If workspace, do crate boundaries match deployment/runtime boundaries?
- Is shared code actually shared, or is it coupled to one side?
- Are front-end-only dependencies leaking into back-end builds?
- Are back-end-only dependencies leaking into front-end or wasm builds?
- Are common types duplicated between front-end and back-end?
- Does the crate graph make sense for a simple app?
```

Healthy patterns:

```text
my_app/
  Cargo.toml                  # workspace
  crates/
    backend/
    frontend/
    shared/
```

Or, for a smaller project:

```text
src/
  main.rs
  backend/
  frontend/
  shared/
```

Potential smells:

```text
- One large `main.rs` owns everything
- Front-end and back-end modules import each other directly
- Shared types live in an arbitrary UI or server module
- API response types are duplicated manually
- Feature flags are confusing or required for normal builds
- Workspace members are named vaguely, such as `core`, `common`, or `utils`, without clear ownership
```

### Pass 2: File Length and Module Complexity

Objectives:

- Find files that are too long
- Find modules with multiple responsibilities
- Find functions that are too long or deeply nested
- Find overly large enums/structs/impl blocks
- Find excessive public APIs

Suggested thresholds for a simple Rust app:

| Item | Good | Caution | Strong Concern |
|---|---:|---:|---:|
| Source file length | < 250 lines | 250-500 lines | > 500 lines |
| Function length | < 40 lines | 40-100 lines | > 100 lines |
| `impl` block length | < 150 lines | 150-300 lines | > 300 lines |
| Module responsibilities | 1 clear concern | 2 related concerns | 3+ unrelated concerns |
| Nesting depth | 1-3 | 4 | 5+ |
| Match arms | clear and grouped | long but readable | repeated logic or nested matches |
| Public items per module | minimal | broad | everything is `pub` |

These are heuristics, not hard rules. A generated file or a declarative route table may be long but acceptable. A hand-written file with many unrelated concepts is a stronger concern.

Prompt:

```text
Audit file and function size. Identify the top 10 longest Rust files and top 10 most complex-looking functions or impl blocks. For each, decide whether the length is justified.

For each oversized file, classify the reason:
- Multiple domains mixed together
- Routes and business logic mixed together
- UI rendering and state mutation mixed together
- Validation and persistence mixed together
- Serialization types mixed with behavior
- Test fixtures mixed with production code
- Repeated code that could be extracted
- Generated or mostly declarative content
- Legitimately cohesive despite size

Recommend specific splits. Do not recommend splitting purely by line count if the file is cohesive.
```

Suggested commands:

```bash
find . -name '*.rs' -not -path '*/target/*' -print0 \
  | xargs -0 wc -l \
  | sort -nr \
  | head -30

# Approximate large functions/impls manually with ripgrep context:
rg '^\s*(pub\s+)?(async\s+)?fn\s+' . --glob '*.rs' --glob '!target/**'
rg '^\s*impl\b' . --glob '*.rs' --glob '!target/**'
```

### Pass 3: Architecture and Responsibility Boundaries

Objectives:

- Identify layers
- Identify whether layers depend in the correct direction
- Identify whether domain logic is isolated from framework logic
- Identify whether front-end and back-end communicate through stable contracts

Prompt:

```text
Analyze the architecture. Describe the current layers and dependency direction. Identify whether the code has the following conceptual boundaries:

- Entry points
- Routing
- Request parsing
- Validation
- Authentication/authorization
- Business/domain logic
- Persistence/external services
- Response mapping
- Shared data contracts
- Front-end routing
- Front-end state management
- Front-end components/views
- Front-end API client
- Styling/assets
- Configuration
- Logging/telemetry

For each boundary, state whether it is:
- Clear and well-owned
- Present but leaky
- Missing but not needed yet
- Missing and causing complexity

Provide a recommended target architecture appropriate for a simple Rust app.
```

Healthy dependency direction:

```text
Back-end route handlers -> services/use cases -> repositories/external adapters
Front-end components -> state/actions -> API client -> shared DTOs
Shared crate/module -> pure types, validation helpers, protocol constants
```

Smells:

```text
- Database queries directly inside route handlers
- HTTP response construction inside domain logic
- UI components directly constructing raw URLs everywhere
- Server-specific types used by front-end shared models
- Front-end state mutation scattered across many components
- Business rules duplicated on client and server with no shared validation strategy
- `utils.rs` becoming a dumping ground
```

### Pass 4: Back-End Review

Objectives:

- Review server entry point
- Review routing
- Review handlers
- Review service/business logic
- Review persistence
- Review validation
- Review authentication and authorization
- Review error mapping
- Review configuration and secrets

Prompt:

```text
Perform a back-end audit. Identify the framework used, entry point, route registration, request/response types, persistence mechanism, configuration, authentication, and external integrations.

For each route or handler group, assess:
- Is the route easy to discover?
- Is the handler small and focused?
- Is validation explicit?
- Are errors mapped consistently?
- Are database/external calls isolated?
- Is authorization enforced in one place or scattered?
- Are response types stable and documented?
- Are tests present for success and failure cases?

Flag any route handlers that mix too many responsibilities.
```

Back-end structure patterns:

```text
backend/
  main.rs or lib.rs
  app.rs              # app/router construction
  routes/
    mod.rs
    health.rs
    users.rs
    items.rs
  handlers/
  services/
  repositories/
  dto.rs
  error.rs
  config.rs
  state.rs
```

For a simple app, `routes` and `handlers` may be combined if handlers remain small.

Back-end smell checklist:

```text
- One file contains all routes and all business logic
- Handlers directly parse environment variables
- Handlers directly create database pools
- Repeated manual JSON response construction
- `unwrap` or `expect` on request data, database results, or external IO
- Inconsistent status codes
- Authorization checks are missing or duplicated
- Sensitive errors are returned to clients
- Domain logic depends on framework-specific extractors
- Test setup is so difficult that handlers are not tested
```

### Pass 5: Front-End Review

Objectives:

- Review Rust front-end framework usage
- Review component structure
- Review state management
- Review API client
- Review routing/navigation
- Review rendering performance
- Review error/loading/empty states
- Review accessibility where applicable

Prompt:

```text
Perform a front-end audit. Identify the Rust front-end technology or framework used, such as Leptos, Yew, Dioxus, Sycamore, egui, iced, Tauri front-end, or another Rust UI stack.

Assess:
- Entry point and app initialization
- Routing and page structure
- Component boundaries
- State ownership
- API client abstraction
- Error/loading/empty states
- Form validation
- Accessibility and semantics where applicable
- Styling strategy
- Asset handling
- WASM-specific build concerns, if applicable
- Hydration/server-side rendering concerns, if applicable

For each major component/page, decide whether it is cohesive or should be split.
```

Front-end structure patterns:

```text
frontend/
  main.rs
  app.rs
  routes.rs
  pages/
    home.rs
    login.rs
    dashboard.rs
  components/
    button.rs
    form_field.rs
    layout.rs
  state/
  api/
    client.rs
    error.rs
  hooks/ or resources/       # framework-dependent
  styles/ or assets/
```

Smells:

```text
- A single `app.rs` contains every page and component
- Components fetch data directly from many different places
- Raw API paths are repeated across components
- UI state and server state are mixed without clear ownership
- No loading state for async fetches
- No error state for failed requests
- Forms duplicate server validation but drift from it
- Component props are huge and unclear
- Deeply nested view macros obscure logic
- Business rules are embedded in rendering code
- `clone()` is used excessively to satisfy ownership without understanding state flow
```

### Pass 6: Shared Types and API Contracts

Objectives:

- Review DTOs/request/response types
- Review shared crate/module
- Review validation consistency
- Review versioning and compatibility

Prompt:

```text
Audit the contract between front-end and back-end.

Identify:
- Request types
- Response types
- Error response format
- Shared enums
- ID types
- Date/time formats
- Serialization rules
- Optional/null fields
- Validation rules
- API path constants or generated clients

Determine whether the front-end and back-end share types safely or duplicate them.

If shared types exist, assess whether they are truly platform-neutral. They should not pull in server-only dependencies into the front-end build, and should not force UI concerns into the server.
```

Healthy shared module contents:

```text
shared/
  dto.rs
  ids.rs
  validation.rs
  api_paths.rs
  errors.rs       # only protocol-level error shapes, not server internals
```

Shared code smells:

```text
- Shared crate imports database types
- Shared crate imports server framework types
- Shared crate imports browser-only UI dependencies
- DTOs contain methods that perform IO
- API errors expose internal server errors
- Client and server define separate versions of the same enum
- Date/time/string formats are implicit
- IDs are raw strings everywhere despite meaningful domain concepts
```

### Pass 7: Rust Idioms and Type Design

Objectives:

- Review type safety
- Review ownership and borrowing
- Review error handling
- Review traits and generics
- Review module visibility
- Review naming
- Review use of `Option`, `Result`, enums, and newtypes

Prompt:

```text
Perform a Rust idiom audit. Focus on whether the code uses Rust's type system to make invalid states hard to represent and whether it avoids unnecessary complexity.

Check:
- Are domain concepts represented by meaningful types?
- Are raw `String`, `u64`, or `Uuid` values overused where newtypes would help?
- Are enums used for closed sets of states?
- Are `Option` and `Result` handled explicitly?
- Are errors modeled clearly?
- Is visibility restricted with `pub(crate)` or private items where possible?
- Are clones justified?
- Are lifetimes/generics simple and useful, or over-engineered?
- Are traits used to enable abstraction/testing, or used prematurely?
- Is unsafe code absent or justified?
- Are macros used sparingly and clearly?
```

Rust smell checklist:

```text
- Repeated `unwrap()` or `expect()` outside startup/test code
- Public fields everywhere without invariants
- Large untyped strings for statuses, roles, routes, or event names
- `bool` parameters that obscure intent
- `Option<Option<T>>` or deeply nested data without explanation
- `Result<T, String>` in production code
- Silent error swallowing with `_ = ...`
- Excessive `clone()` in hot paths or state updates
- Large match statements with repeated arms
- `todo!()`, `unimplemented!()`, or `panic!()` in reachable production paths
- `pub use` re-export maze that obscures ownership
- Overly generic functions where concrete types would be clearer
- Traits with only one implementation and no test seam need
```

Preferred Rust patterns:

```rust
pub struct UserId(uuid::Uuid);

pub enum UserRole {
    Admin,
    Member,
    Guest,
}

pub enum AppError {
    NotFound,
    Validation(String),
    Unauthorized,
    Internal(anyhow::Error),
}
```

The reviewer should adapt these ideas to the actual project rather than forcing them everywhere.

### Pass 8: Error Handling

Objectives:

- Determine whether errors are meaningful
- Determine whether internal errors are hidden from users
- Determine whether logs preserve debugging context
- Determine whether front-end user-facing errors are useful

Prompt:

```text
Audit error handling across the application.

Back-end:
- Identify the central error type, if any
- Check mapping from internal errors to HTTP status codes
- Check whether sensitive details are hidden from clients
- Check whether errors preserve source context
- Check whether validation errors are structured
- Check whether handler errors are consistent

Front-end:
- Check whether API errors are parsed consistently
- Check whether user-facing messages are clear
- Check whether loading, empty, retry, and failed states are handled
- Check whether unexpected errors are surfaced or silently ignored

Produce a table of error flows from source to user/log.
```

Potential good patterns:

```text
- `thiserror` for domain/application error enums
- `anyhow` for application startup or top-level internal context
- Framework-specific response conversion in one place
- Structured API error shape shared with front-end
- User-facing messages separated from internal logs
```

Potential smells:

```text
- `unwrap()` on request, DB, filesystem, or network operations
- Returning raw database errors in JSON responses
- Mapping every error to `500 Internal Server Error`
- Mapping validation errors to `500`
- Front-end displays `Debug` output to users
- Errors logged without request context
- Errors swallowed and replaced by default values
```

### Pass 9: Async, Concurrency, and Runtime Behavior

Objectives:

- Review async correctness
- Review blocking calls
- Review shared state
- Review locks
- Review task spawning
- Review cancellation/timeouts

Prompt:

```text
If the application uses async Rust, audit runtime behavior.

Check:
- Which async runtime is used?
- Are blocking operations performed inside async handlers?
- Are locks held across `.await`?
- Are shared states wrapped appropriately?
- Are background tasks supervised?
- Are spawned tasks awaited, tracked, or intentionally detached?
- Are timeouts used for external calls?
- Are cancellation and shutdown considered?
- Are streams handled safely?
- Is front-end async state handled without race conditions?
```

Smells:

```text
- `std::thread::sleep` in async code
- File, network, or CPU-heavy work directly inside async handlers without offloading
- `MutexGuard` held across `.await`
- Unbounded channels without backpressure
- Detached tasks that can fail silently
- Shared mutable state used as an ad hoc database
- Missing timeout around external requests
- Front-end fetch result can overwrite newer state after navigation
```

### Pass 10: Security Review

Objectives:

- Identify common web security flaws
- Review authentication and authorization
- Review input validation
- Review secret handling
- Review dependency risk
- Review front-end exposure

Prompt:

```text
Perform a security audit appropriate for a simple Rust full-stack application.

Check:
- Authentication flow
- Authorization checks
- Session/cookie/token handling
- CSRF protections if cookies are used
- CORS configuration
- Input validation and output encoding
- SQL injection or query construction risk
- Path traversal risk
- SSRF risk if server fetches user-provided URLs
- XSS risk in front-end rendering
- Open redirects
- Rate limiting or abuse protection for sensitive routes
- Password handling if applicable
- Secret management
- Logging of sensitive data
- Dependency vulnerabilities
- Debug endpoints or dev-only behavior exposed in production
```

Security issue format:

```text
### Finding: [short title]
Severity: High
Category: Security
Evidence: `path/to/file.rs`, function `name`
Risk: Describe concrete exploit or failure mode.
Recommendation: Describe precise mitigation.
Validation: Tests or commands to confirm.
```

Security smells:

```text
- `CorsLayer::permissive()` used in production
- Cookies missing `HttpOnly`, `Secure`, or `SameSite`
- JWT accepted without validating issuer/audience/expiration
- Passwords stored or compared directly
- Secrets read from source-controlled config
- User-controlled path joined to filesystem path without normalization
- Raw SQL assembled with `format!`
- Error responses reveal stack traces, SQL errors, or internal paths
- Logs include tokens, passwords, session IDs, or full authorization headers
- Front-end stores long-lived sensitive tokens in local storage without justification
```

### Pass 11: Dependency and Build Audit

Objectives:

- Review dependency scope
- Review feature flags
- Review compile target separation
- Review vulnerability and license checks
- Review binary size concerns for WASM/front-end

Prompt:

```text
Audit dependencies and build configuration.

Check:
- Workspace dependencies organization
- Feature flags
- Default features
- Back-end-only dependencies
- Front-end-only dependencies
- Shared crate dependencies
- Duplicate dependency versions
- Unused dependencies
- Heavy dependencies in WASM target
- Security advisories
- License policy, if relevant
- Build scripts and generated code
- Release profile
```

Suggested commands if available:

```bash
cargo tree --workspace
cargo tree --workspace -d
cargo metadata --format-version=1
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
cargo deny check
cargo machete
cargo udeps --workspace --all-targets
```

Dependency smells:

```text
- Shared crate depends on web framework, database client, or UI framework
- Front-end pulls in large server-only dependencies
- Multiple versions of major libraries without reason
- Many dependencies for trivial functionality
- Feature flags required in surprising combinations
- `default-features = true` accidentally pulls in unwanted runtime features
- Build.rs performs network access or fragile environment assumptions
```

### Pass 12: Testing Review

Objectives:

- Assess coverage by behavior, not only percentage
- Review unit/integration/e2e tests
- Review test maintainability
- Review fixtures and helpers
- Review front-end testing

Prompt:

```text
Audit the testing strategy.

Identify:
- Unit tests
- Integration tests
- API tests
- Front-end/component tests
- Serialization contract tests
- Validation tests
- Error-path tests
- Security-sensitive tests
- Database tests
- Snapshot tests, if any
- Property-based tests, if any
- Test helpers/fixtures

Assess whether critical behavior is covered. Do not rely only on coverage percentage.

For each major module, state:
- Existing tests
- Missing tests
- Highest-value tests to add first
- Whether the code is easy to test
```

Suggested commands:

```bash
cargo test --workspace --all-features
cargo test --workspace --all-targets
cargo nextest run --workspace --all-features
cargo tarpaulin --workspace --all-features
cargo llvm-cov --workspace --all-features
```

Test smells:

```text
- No tests for route handlers
- Only happy-path tests
- Tests rely on global state or real external services
- Database tests are order-dependent
- Serialization contracts are untested
- Front-end API error states are untested
- Tests duplicate implementation details instead of behavior
- Test names do not explain behavior
- Large integration tests are the only tests
- Flaky async sleeps instead of deterministic synchronization
```

Preferred test naming:

```rust
#[test]
fn rejects_empty_username() {
    // ...
}

#[tokio::test]
async fn create_user_returns_conflict_when_email_already_exists() {
    // ...
}
```

### Pass 13: Performance Review

Objectives:

- Review obvious inefficiencies
- Review async blocking
- Review database query patterns
- Review unnecessary cloning/allocation
- Review WASM bundle size and front-end rendering

Prompt:

```text
Perform a performance audit. Focus on practical risks, not speculative micro-optimizations.

Back-end:
- Blocking operations in async paths
- N+1 database queries
- Missing indexes or inefficient queries, if visible
- Repeated expensive parsing/config loading
- Large response allocations
- Unbounded request payloads
- Excessive cloning in hot paths
- Connection pool configuration

Front-end:
- WASM binary size
- Large components rerendering too often
- Excessive cloning in reactive state
- Repeated network requests
- Missing pagination or lazy loading
- Large serialized payloads
- Inefficient derived state

Return only performance findings with plausible user impact.
```

Performance smells:

```text
- Database pool created per request
- HTTP client created per request
- Full table loaded to filter in memory
- No pagination for list endpoints
- Large JSON cloned repeatedly
- Front-end fetches data repeatedly on every render
- Large dependencies pulled into WASM for small utilities
- `debug` logging formats expensive data in hot paths
```

### Pass 14: Observability and Operations Review

Objectives:

- Review logs
- Review tracing
- Review health checks
- Review metrics
- Review operational errors

Prompt:

```text
Audit observability and operational readiness.

Check:
- Structured logging/tracing
- Request IDs/correlation IDs
- Error logs include useful context
- Sensitive data is redacted
- Health check endpoint
- Readiness check for dependencies
- Metrics, if appropriate
- Panic behavior
- Startup logs
- Configuration visibility without secrets
- Graceful shutdown
```

Smells:

```text
- `println!` used instead of structured logging in server code
- Errors are returned but not logged where appropriate
- Logs include full request bodies with secrets
- No health endpoint
- Health endpoint always returns OK without checking dependencies
- Startup failure messages are vague
- Panics in request handlers
```

### Pass 15: Documentation and Developer Experience

Objectives:

- Assess whether a new developer can build, run, test, and understand the app
- Review README, comments, module docs, examples, and commands

Prompt:

```text
Audit documentation and developer experience.

Check:
- README explains what the app does
- Local setup is documented
- Required environment variables are documented
- Commands for back-end and front-end are documented
- Database setup/migrations are documented
- Testing commands are documented
- Deployment/release process is documented
- Architecture overview exists or is inferable
- Public APIs have useful rustdoc where appropriate
- Comments explain why, not just what
```

DX smells:

```text
- README only says how to install dependencies but not how to run app
- Environment variables are used but not listed
- `.env.example` missing or incomplete
- Front-end build command is undocumented
- Tests require services that are not documented
- Module names do not reveal purpose
- Comments restate code instead of explaining decisions
```

### Pass 16: CI, Formatting, and Release Review

Objectives:

- Verify formatting and linting
- Verify tests in CI
- Verify security checks
- Verify release/build consistency

Prompt:

```text
Audit CI and release configuration.

Check:
- Does CI run `cargo fmt --check`?
- Does CI run clippy with useful settings?
- Does CI run tests?
- Does CI build both back-end and front-end targets?
- Are wasm/front-end builds tested if applicable?
- Are dependency/security checks included?
- Are caches configured safely?
- Are release profiles reasonable?
- Are artifacts generated reproducibly?
- Are deployment secrets handled safely?
```

Suggested baseline CI checks:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo check --workspace --all-targets --all-features
```

Optional checks:

```bash
cargo audit
cargo deny check
cargo nextest run --workspace --all-features
cargo llvm-cov --workspace --all-features
```

---

## 6. File and Module Organization Rubric

Use this rubric to decide whether the codebase is well divided.

### 6.1 A Well-Divided Codebase Has

```text
- Entry points that only initialize and delegate
- Route definitions separated from substantial business logic
- Domain logic independent of HTTP/UI frameworks where practical
- Persistence isolated behind small modules or traits where useful
- Shared DTOs in a neutral location
- Front-end components grouped by page/domain/shared component status
- API client code centralized on the front-end
- Configuration parsed once and passed explicitly
- Error types defined close to the layer that owns them
- Tests near the behavior they validate or in clear integration folders
```

### 6.2 A Poorly Divided Codebase Often Has

```text
- `main.rs` doing initialization, routing, logic, database calls, and rendering
- One `lib.rs` exporting everything
- A huge `utils.rs` file with unrelated helpers
- A huge `types.rs` file containing all structs in the project
- `models.rs` mixing database rows, domain objects, and API DTOs
- Front-end components directly importing back-end internals
- Back-end code importing front-end component types
- Repeated conversion code scattered across handlers
- Public modules that should be private
- Circular conceptual dependencies even if Rust module dependencies are acyclic
```

### 6.3 Suggested Split Criteria

A file should usually be split when two or more of these are true:

```text
- It has more than one primary reason to change
- It contains both infrastructure and domain behavior
- It contains both UI rendering and networking logic
- It contains route handlers for unrelated resources
- It contains test helpers mixed with production code
- It has repeated internal sections separated by comments that could be modules
- It requires scrolling a lot to understand one concept
- It has many unrelated imports
- It exposes many public items because internal boundaries are unclear
```

A file may remain large when:

```text
- It is generated
- It is mostly declarative data
- It is a cohesive list of routes or constants
- Splitting would create tiny artificial files
- The framework encourages colocation and the file remains readable
```

### 6.4 Specific Rust Module Advice

Check whether the project uses modern Rust module conventions clearly.

Acceptable:

```text
src/foo.rs
src/foo/bar.rs
```

Also acceptable:

```text
src/foo/mod.rs
src/foo/bar.rs
```

But avoid mixing styles inconsistently unless there is a reason.

Check:

```text
- Are module names domain-specific?
- Are modules small enough to understand?
- Are `pub mod` declarations necessary?
- Could some modules be private?
- Are re-exports helpful or confusing?
- Do file names match the primary type or concept inside?
```

---

## 7. Back-End-Specific Audit Checklist

Use this checklist for back-end source review.

### 7.1 Entry Point

```text
- Does `main` do only startup work?
- Is configuration loaded once?
- Is logging/tracing initialized early?
- Is app/router construction delegated?
- Are database pools or clients constructed once?
- Is shutdown handled gracefully if relevant?
- Are startup failures reported clearly?
```

### 7.2 Routing

```text
- Are routes grouped logically?
- Are route paths constants or discoverable?
- Are HTTP methods appropriate?
- Are route handlers named consistently?
- Are health/status routes separated from business routes?
- Are admin/private routes protected clearly?
```

### 7.3 Handlers

```text
- Is each handler short and focused?
- Does it parse input, call a service, and map output?
- Is validation explicit?
- Does it avoid direct database logic when service logic is nontrivial?
- Does it avoid leaking internal errors?
- Does it have tests?
```

### 7.4 Services or Domain Logic

```text
- Are business rules centralized?
- Are invariants enforced by types or constructors?
- Are side effects explicit?
- Are services easy to test?
- Is framework-specific code avoided inside domain logic?
```

### 7.5 Persistence

```text
- Are queries parameterized?
- Is connection pooling used appropriately?
- Are migrations present if using a database?
- Are transaction boundaries clear?
- Are database row types separated from API DTOs if needed?
- Are repository functions named by intent, not query mechanics?
```

### 7.6 Configuration

```text
- Are required environment variables documented?
- Is config parsed into typed structs?
- Are defaults explicit?
- Are secrets excluded from logs?
- Are dev/test/prod differences clear?
```

---

## 8. Front-End-Specific Audit Checklist

Use this checklist for the Rust front-end.

### 8.1 App Entry and Routing

```text
- Is the front-end entry point small?
- Is routing centralized or clearly discoverable?
- Are pages separated from reusable components?
- Are route parameters parsed safely?
- Are navigation errors handled?
```

### 8.2 Components

```text
- Does each component have a clear purpose?
- Are large components split into smaller subcomponents?
- Are props minimal and meaningful?
- Are callbacks/actions named clearly?
- Is business logic extracted from rendering macros/templates?
- Are repeated UI patterns extracted only when reuse is real?
```

### 8.3 State Management

```text
- Is local state kept local?
- Is shared state intentionally shared?
- Is server state distinguished from UI state?
- Are loading/error/success states explicit?
- Are stale async responses handled?
- Are derived values computed cleanly?
```

### 8.4 API Client

```text
- Is there a centralized API client?
- Are raw endpoints avoided in components?
- Are request/response DTOs shared or generated?
- Are errors parsed consistently?
- Are retries/timeouts used where appropriate?
- Are auth headers/cookies handled consistently?
```

### 8.5 UX States

```text
- Loading state
- Empty state
- Error state
- Retry path
- Disabled/submitting state for forms
- Validation feedback
- Success confirmation
- Unauthorized/session-expired state
```

### 8.6 Accessibility and Semantics

Even Rust-generated UI should produce accessible output when targeting the web.

```text
- Buttons are real buttons, not clickable divs
- Inputs have labels
- Errors are associated with fields
- Keyboard navigation works
- Focus management is reasonable
- Images/icons have accessible labels or are hidden appropriately
- Color is not the only signal
```

---

## 9. API Boundary Review

The front-end/back-end boundary is often where simple full-stack apps become brittle. The LLM should inspect it carefully.

### 9.1 Contract Questions

```text
- Where are API request and response types defined?
- Are they shared between front-end and back-end?
- If shared, are dependencies target-neutral?
- If duplicated, how is drift prevented?
- Is there a consistent error response shape?
- Are routes and methods documented?
- Are serialization names stable?
- Are optional fields intentional?
- Are breaking changes tested?
```

### 9.2 DTO Review

```text
- Are DTOs distinct from database rows?
- Are DTOs distinct from internal domain objects when needed?
- Are field names stable and intentional?
- Are serde attributes documented where non-obvious?
- Are default values safe?
- Are dates/times timezone-aware where needed?
```

### 9.3 Example Contract Finding

```text
Finding: API response shape is duplicated between client and server
Severity: Medium
Category: API Contract
Evidence: `crates/backend/src/users.rs` defines `UserResponse`; `crates/frontend/src/api.rs` defines a structurally similar `UserResponse`.
Why it matters: Changes to one side can silently break the other side.
Recommended fix: Move protocol DTOs to `crates/shared/src/dto.rs` or generate the client types from a schema.
Concrete steps:
1. Create `shared::dto::UserResponse`.
2. Update server handler to return it.
3. Update front-end API client to deserialize it.
4. Add a serialization round-trip test.
Validation: Run `cargo test --workspace --all-features`.
```

---

## 10. Long File Review Template

For every oversized file or dense module, use this template.

```text
### File: `path/to/file.rs`

Line count: N
Primary responsibility: ...
Secondary responsibilities found:
- ...
- ...

Assessment:
- Cohesion: High / Medium / Low
- Split urgency: High / Medium / Low / Not needed
- Reason length may be acceptable: ...
- Reason length is harmful: ...

Evidence:
- `function_name` handles ...
- `struct_name` represents ...
- `impl Foo` also performs ...

Recommended split:
- Move ... to `...`
- Move ... to `...`
- Keep ... in this file

Suggested order:
1. Extract pure types first
2. Extract helper functions second
3. Extract side-effecting adapters third
4. Add tests around behavior before changing logic

Validation:
- `cargo fmt`
- `cargo test ...`
- `cargo clippy ...`
```

---

## 11. Issue Severity Rubric

Use consistent severity definitions.

### Blocker

A defect or risk that prevents safe release or further review.

Examples:

```text
- Code does not compile
- Tests cannot run due to missing critical setup
- Authentication is completely bypassed
- Production secrets committed to repository
- Data loss risk in normal usage
```

### High

Likely to cause bugs, security issues, production failures, or major maintenance cost.

Examples:

```text
- Authorization missing on sensitive route
- Route handlers expose internal errors and stack traces
- Database writes are not transactional where required
- Front-end and back-end API types have already drifted
- Large module mixes unrelated layers and is blocking safe changes
```

### Medium

Real maintainability, correctness, or scalability concern, but not immediately dangerous.

Examples:

```text
- Oversized cohesive file that should be split soon
- Repeated validation logic
- Inconsistent error mapping
- Missing tests for important failure paths
- API client scattered across components
```

### Low

Useful improvement with modest impact.

Examples:

```text
- Naming inconsistencies
- Minor duplication
- Missing docs for non-public helper
- Small opportunity to simplify type signatures
```

### Nit

Style-level feedback that should not distract from important issues.

Examples:

```text
- Import ordering if rustfmt does not handle it
- Slightly clearer variable name
- Small comment improvement
```

---

## 12. Findings Output Format

The LLM should use this format for findings.

```markdown
## Finding N: Short, Specific Title

**Severity:** Medium  
**Category:** Architecture / Modularity / Security / Testing / etc.  
**Evidence:** `path/to/file.rs`, `function_or_type_name`  

### What I found
Describe the issue factually.

### Why it matters
Explain the concrete maintenance, correctness, security, or performance impact.

### Recommended fix
Give the best practical fix for this codebase size.

### Concrete steps
1. ...
2. ...
3. ...

### Validation
- `cargo test ...`
- `cargo clippy ...`
- Add/modify test: ...

### Risk of change
Low / Medium / High, with reason.
```

---

## 13. Prioritized Findings Table

The review should include a table like this.

```markdown
| Priority | Severity | Category | Location | Finding | Recommended Action |
|---:|---|---|---|---|---|
| 1 | High | Security | `...` | ... | ... |
| 2 | High | Error Handling | `...` | ... | ... |
| 3 | Medium | Modularity | `...` | ... | ... |
```

Priority should combine severity, likelihood, user impact, and ease of improvement.

---

## 14. Refactoring Roadmap Template

The LLM should provide a staged roadmap instead of an unstructured list.

```markdown
## Refactoring Roadmap

### Stage 0: Safety Net

Goal: Make future refactors safe.

Actions:
- Add missing characterization tests around current behavior
- Ensure `cargo fmt`, `cargo clippy`, and `cargo test` pass
- Add API serialization tests for shared DTOs

Exit criteria:
- Tests pass consistently
- Existing behavior is documented by tests

### Stage 1: Boundary Cleanup

Goal: Separate the most tangled responsibilities without changing behavior.

Actions:
- Extract DTOs
- Extract API client
- Extract route handlers by resource
- Extract service functions from handlers

Exit criteria:
- Public behavior unchanged
- Files are smaller and responsibilities clearer

### Stage 2: Error and Validation Consistency

Goal: Standardize failure behavior.

Actions:
- Introduce/clean central error type
- Map errors to stable API shape
- Share validation where appropriate
- Add error-path tests

Exit criteria:
- Common errors have consistent status codes/messages
- Front-end handles API errors consistently

### Stage 3: Test and CI Hardening

Goal: Prevent regression.

Actions:
- Add CI checks
- Add integration tests for important routes
- Add front-end state/API tests if framework supports them
- Add dependency/security checks

Exit criteria:
- CI validates format, lint, test, and build targets

### Stage 4: Optional Improvements

Goal: Improve performance, observability, and documentation.

Actions:
- Add structured tracing
- Add health/readiness checks
- Add docs for local setup and architecture
- Reduce unnecessary front-end/back-end dependency weight
```

---

## 15. Quick Wins Template

The LLM should identify improvements that can be done in less than a day.

```markdown
## Quick Wins

1. Move raw API path strings into a single `api_paths` module.
2. Add `.env.example` documenting required variables.
3. Replace production `unwrap()` calls in handlers with proper error propagation.
4. Split the largest component into page/container and presentational components.
5. Add `cargo fmt --check`, `cargo clippy`, and `cargo test` to CI.
6. Add shared API error DTO and parse it consistently in front-end client.
7. Restrict unnecessary `pub` items to `pub(crate)` or private.
8. Add tests for validation and error paths.
```

The actual quick wins must be based on the inspected repository, not copied blindly.

---

## 16. Scoring Rubric

Optionally ask the LLM to score the codebase. Scores should be justified with evidence.

```markdown
## Codebase Health Scorecard

| Area | Score 1-10 | Rationale | Highest-Impact Improvement |
|---|---:|---|---|
| Architecture | | | |
| Module Boundaries | | | |
| File Size/Cohesion | | | |
| Rust Idioms | | | |
| Error Handling | | | |
| Back-End Quality | | | |
| Front-End Quality | | | |
| API Contract | | | |
| Security | | | |
| Testing | | | |
| Performance | | | |
| Observability | | | |
| Documentation/DX | | | |
| CI/Release | | | |
```

Scoring guide:

```text
10: Excellent; few meaningful improvements
8-9: Strong; minor or localized issues
6-7: Functional but has noticeable maintainability risks
4-5: Works but structural issues will slow development
2-3: Fragile; high risk of bugs or unsafe changes
1: Failing or unsafe foundation
```

---

## 17. Special Attention Areas for Rust Full-Stack Apps

### 17.1 WASM and Target Boundaries

If the front-end compiles to WASM, check:

```text
- Shared crates compile for wasm target
- Server-only dependencies are not pulled into wasm
- Feature flags separate native and wasm behavior
- Time, random, networking, and filesystem APIs are target-compatible
- Panic hooks and logging are configured appropriately for development
- Release build size is considered
```

Smells:

```text
- Shared crate depends on Tokio features unavailable or unnecessary in browser
- Shared crate imports server framework extractors/responders
- Front-end build breaks unless unrelated server features are enabled
- Large transitive dependencies inflate WASM binary
```

### 17.2 Server-Side Rendering or Hydration

If using SSR/hydration frameworks, check:

```text
- Server-only code is gated correctly
- Hydration mismatches are avoided
- Data fetching is not duplicated unnecessarily
- Secrets are never included in serialized client state
- Feature flags are clear
```

### 17.3 Tauri/Desktop Front-End

If the Rust front-end is part of a desktop app, check:

```text
- Command boundary between UI and backend is clear
- Permissions/capabilities are minimal
- File system access is constrained
- User input is validated before privileged operations
- Long-running work does not block UI
```

### 17.4 Database and Migration Boundaries

If a database is present, check:

```text
- Migrations are source-controlled
- Migration command is documented
- Schema and Rust types align
- Query errors are handled correctly
- Transactions protect multi-step writes
- Tests can run against isolated database state
```

---

## 18. Anti-Patterns to Actively Search For

### 18.1 Architectural Anti-Patterns

```text
- God `main.rs`
- God `app.rs`
- God `state.rs`
- God `utils.rs`
- Mixed UI/server/domain module
- Framework lock-in throughout domain logic
- Shared crate with non-shared concerns
- Multiple sources of truth for API shape
```

### 18.2 Rust Anti-Patterns

```text
- `unwrap()` in production path
- `expect()` with vague message in production path
- `Result<T, String>` for structured application errors
- Overuse of `Arc<Mutex<...>>`
- Lock held across await
- `clone()` used to avoid designing ownership/state flow
- Excessive `pub`
- Giant enums with unrelated variants
- Empty marker traits with unclear purpose
- Dynamic dispatch where static dispatch or concrete types are simpler
- Static dispatch/generics where dynamic dispatch would simplify tests and compile times
```

### 18.3 Back-End Anti-Patterns

```text
- Direct SQL in every handler
- No central error-to-response mapping
- Inconsistent status codes
- Inconsistent validation
- No request size limits
- No timeout for external calls
- Permissive CORS in production
- Auth middleware exists but routes bypass it
```

### 18.4 Front-End Anti-Patterns

```text
- One component owns an entire application page tree
- Business logic hidden inside render/template macros
- Fetching from components without API abstraction
- Raw endpoint strings repeated throughout UI
- Missing loading/error states
- Repeated local copies of server state
- Shared mutable state without clear ownership
- Accessibility ignored because UI is generated from Rust
```

---

## 19. Command Review Prompt

If the LLM can run commands, use this prompt.

```text
Run non-destructive inspection commands only. Do not modify the repository yet.

Run the safest relevant commands from this list, depending on what exists:

1. Repository shape:
   - `find . -maxdepth 4 -type f | sort`
   - `find . -name '*.rs' -not -path '*/target/*' -print | sort`

2. Rust metadata:
   - `cargo metadata --format-version=1`
   - `cargo tree --workspace`
   - `cargo tree --workspace -d`

3. Build/lint/test:
   - `cargo fmt --all -- --check`
   - `cargo check --workspace --all-targets --all-features`
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo test --workspace --all-features`

4. Optional security/dependency tools only if installed:
   - `cargo audit`
   - `cargo deny check`
   - `cargo machete`
   - `cargo udeps --workspace --all-targets`

5. Front-end-specific commands only if the project uses them:
   - `trunk build`
   - `trunk build --release`
   - `wasm-pack test --headless --chrome`
   - `dx build` for Dioxus projects if configured
   - framework-specific test/build commands documented in README

For every command:
- Report whether it passed or failed
- Include the most relevant error lines only
- Explain whether failure blocks review or simply informs findings
- Do not treat missing optional tools as codebase failures
```

---

## 20. Review Output Skeleton

Ask the LLM to produce the final review using this structure.

```markdown
# Rust Codebase Review

## 1. Executive Summary

- Overall assessment:
- Main risks:
- Best qualities:
- Highest-priority recommendation:

## 2. Repository Map

```text
...
```

## 3. Architecture Overview

Describe current architecture and dependency direction.

## 4. Major Findings

### Finding 1: ...
...

## 5. File and Module Organization

### Oversized files
| File | Lines | Assessment | Recommendation |
|---|---:|---|---|

### Boundary concerns
...

## 6. Back-End Review

...

## 7. Front-End Review

...

## 8. Shared/API Contract Review

...

## 9. Rust Idioms Review

...

## 10. Error Handling Review

...

## 11. Security Review

...

## 12. Testing Review

...

## 13. Performance Review

...

## 14. Observability Review

...

## 15. Dependencies and Build Review

...

## 16. Documentation and Developer Experience

...

## 17. Prioritized Findings

| Priority | Severity | Category | Location | Finding | Action |
|---:|---|---|---|---|---|

## 18. Refactoring Roadmap

...

## 19. Quick Wins

...

## 20. Validation Checklist

...

## 21. Open Questions

Only include questions that materially affect the review.
```

---

## 21. Validation Checklist for the LLM’s Review

After producing the review, the LLM should self-check the quality of its own output.

```text
Before finalizing, verify:

- Did I cite actual files/symbols for each finding?
- Did I separate fact from inference?
- Did I avoid inventing behavior?
- Did I review both back-end and front-end?
- Did I review their shared/API boundary?
- Did I assess whether files are too long or poorly divided?
- Did I distinguish severity levels consistently?
- Did I provide actionable fixes?
- Did I avoid recommending unnecessary enterprise architecture?
- Did I identify quick wins?
- Did I provide a staged roadmap?
- Did I include validation commands/tests?
- Did I mention uncertainty where evidence was missing?
```

---

## 22. Mini-Prompts for Focused Follow-Up Reviews

Use these smaller prompts after the initial review.

### 22.1 Focused Modularity Review

```text
Review only module boundaries and file organization. Ignore minor style issues unless they indicate deeper boundary problems.

For each major module/file, answer:
- What responsibility does it appear to own?
- Is that responsibility cohesive?
- What unrelated responsibilities are present?
- What should be extracted?
- What should remain?
- What visibility should be reduced?
- What tests are needed before extraction?
```

### 22.2 Long File Refactoring Review

```text
Analyze `path/to/file.rs` for refactoring. Do not change behavior.

Return:
- Responsibility inventory
- Dependency inventory
- Functions/types that belong together
- Suggested module split
- Exact sequence of safe extraction commits
- Tests to add before and after refactoring
- Risks and mitigations
```

### 22.3 API Contract Review

```text
Review only the API contract between the Rust front-end and Rust back-end.

Find:
- Duplicated request/response types
- Inconsistent error shapes
- Raw endpoint string duplication
- Serialization risks
- Validation drift
- Missing contract tests
- Shared crate dependency leaks

Return prioritized fixes.
```

### 22.4 Security Review

```text
Perform a security-focused review only. Treat this as a web application unless evidence shows otherwise.

Review:
- Authn/authz
- Input validation
- Output encoding
- CORS/CSRF
- Session/token handling
- Secret handling
- Logging
- SQL/path/command injection
- Dependency advisories
- Debug/dev behavior in production

Return only security findings with severity and evidence.
```

### 22.5 Testing Strategy Review

```text
Review the test suite and testability of the codebase.

Return:
- Existing test inventory
- Missing high-value tests
- Modules that are hard to test and why
- Refactors that would improve testability
- Suggested first 10 tests to add
- CI recommendations
```

---

## 23. Suggested First 10 Tests to Ask the LLM to Look For

The exact tests depend on the app, but the LLM should usually check for these categories.

```text
1. Back-end health route returns expected status
2. Main happy-path API route succeeds with valid input
3. Invalid input returns validation error, not 500
4. Unauthorized request is rejected
5. Authorized request with insufficient permissions is rejected
6. Duplicate/conflicting create request returns conflict, if applicable
7. Front-end API client parses success response
8. Front-end API client parses error response
9. Shared DTO serializes/deserializes as expected
10. Critical form or state transition handles loading/error/success states
```

---

## 24. Suggested Refactoring Commit Strategy

The LLM should recommend small commits.

```text
1. Add tests around current behavior
2. Rename unclear modules/files without logic changes
3. Extract shared DTOs without behavior changes
4. Centralize API paths/client calls
5. Extract service/domain functions from handlers
6. Introduce central error mapping
7. Split large UI components
8. Reduce public visibility
9. Remove duplication
10. Add CI checks
```

Each commit should compile and pass tests.

---

## 25. Red Flags That Should Change the Review Priority

If any of these appear, the LLM should elevate priority.

```text
- The code does not compile
- There are no tests at all
- Production paths contain many `unwrap()` calls
- Auth is present but inconsistent
- Sensitive data is logged or committed
- Front-end/back-end contracts are duplicated and already inconsistent
- One file contains most of the application
- Shared crate prevents clean target builds
- CI is absent for a codebase intended for ongoing development
- Database writes can partially succeed without transaction where consistency matters
```

---

## 26. Final Instruction to the LLM Reviewer

End the prompt with this instruction:

```text
Be thorough, specific, and practical. The goal is to help the maintainers make the codebase easier to understand and safer to change. Prefer concrete evidence and staged improvements over broad opinions. If the application is small, recommend simple boundaries; if it is growing, recommend boundaries that can evolve. Do not assume complexity that is not present, but do not ignore early warning signs such as oversized files, duplicated contracts, unclear ownership, scattered state, or inconsistent error handling.
```

---

## 27. Copy-Paste All-In-One Audit Prompt

The following is a condensed all-in-one version suitable for direct use.

```text
You are a principal Rust engineer and full-stack codebase reviewer. Audit this Rust application, which has a simple Rust back-end and Rust front-end. Review architecture, module boundaries, file lengths, Rust idioms, error handling, front-end/back-end contract design, testing, security, performance, observability, dependencies, CI, and documentation.

Rules:
- Be evidence-based. Cite files, symbols, modules, tests, config, or command output.
- Do not invent behavior or files.
- Separate facts, inferences, and recommendations.
- Distinguish serious issues from stylistic preferences.
- Recommend simple, appropriate structure; do not over-engineer a simple app.
- Prefer incremental refactoring and tests before behavior changes.

First, produce a repository map:
- Workspace/crates
- Back-end entry points
- Front-end entry points
- Shared/domain modules
- Tests
- Config/build/CI files
- Large files
- Suspicious modules

Then review in these passes:
1. Repository and crate boundaries
2. File length and module complexity
3. Architecture and dependency direction
4. Back-end routes, handlers, services, persistence, config
5. Front-end routing, components, state, API client, UX states
6. Shared DTOs/API contracts
7. Rust idioms and type design
8. Error handling
9. Async/concurrency, if applicable
10. Security
11. Dependencies and build config
12. Testing
13. Performance
14. Observability/logging
15. Documentation/developer experience
16. CI/release

For file length, use these heuristics:
- <250 lines: usually fine
- 250-500 lines: inspect cohesion
- >500 lines: strong concern unless generated/declarative/cohesive
- Functions >100 lines are strong concerns unless clearly justified
- Split files based on responsibility, not line count alone

For each finding, use this format:
- Severity: Blocker, High, Medium, Low, Nit
- Category
- Evidence
- What I found
- Why it matters
- Recommended fix
- Concrete steps
- Validation commands/tests
- Risk of change

Make sure to answer:
- Are back-end and front-end separated cleanly?
- Are shared types in the right place?
- Are API contracts stable and tested?
- Are any files too long or doing too much?
- Is domain logic separated from framework/UI code?
- Is error handling consistent?
- Are auth/security concerns handled safely?
- Is the app easy to test and refactor?
- Is the project easy for a new developer to build and run?

Final output structure:
1. Executive summary
2. Repository map
3. Architecture assessment
4. File/module organization assessment
5. Back-end review
6. Front-end review
7. Shared/API contract review
8. Rust idioms review
9. Error handling review
10. Security review
11. Testing review
12. Performance review
13. Observability review
14. Dependencies/build review
15. Documentation/DX review
16. Prioritized findings table
17. Refactoring roadmap
18. Quick wins
19. Validation checklist
20. Open questions only if necessary

Be extremely thorough, specific, and practical.
```
