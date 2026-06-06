# Stedi API groups — what each does and key operations

Orientation for picking the right API. This is a guide, not the source of truth —
always confirm exact ids, parameters, and bodies with `stedi ops --search` and
`stedi describe`, since the embedded specs can change between releases. Operation
counts below are approximate.

Run `stedi apis` for the live list with servers and counts.

---

## healthcare — the main clinical/EDI transactions API (~22 ops)

The one you'll use most. Covers the real-time and batch transactions providers
care about.

- **Eligibility (270/271):** `EligibilityCheck`, `EligibilityRawX12Check`
- **Claim status (276/277):** `ClaimStatus`, `ClaimStatusRawX12`
- **Claims submission (837):**
  - Professional: `ClaimsSubmission`, `ClaimsRawX12Submission`
  - Institutional: `InstitutionalClaimsSubmission`, `InstitutionalClaimsRawX12Submission`
  - Dental: `DentalClaimsSubmission`, `DentalClaimsRawX12Submission`
- **Reports / remits:** `ConvertReport277`, `ConvertReport835`,
  `GetElectronicRemittanceAdvicePdf`, `GetPDF1500`, `ExportPDF`
- **Insurance discovery:** `InsuranceDiscoveryCheck`, `GetInsuranceDiscoveryCheck`
- **Coordination of benefits:** `CoordinationOfBenefits`
- **Payer directory (also here):** `ListPayerRecords`, `SearchPayers`,
  `GetPayerRecord`, `ListPayerRecordsCsv`

## payers — the payer directory (~4 ops)

Look up Stedi payer IDs and what each payer supports. Same payer operations also
exist in `healthcare`, so these ids are ambiguous — qualify as
`payers:GetPayerRecord` etc.

- `ListPayerRecords`, `SearchPayers`, `GetPayerRecord`, `ListPayerRecordsCsv`

## manager — eligibility manager / batch eligibility (~5 ops)

Run and track large batches of eligibility checks.

- `BatchEligibilityChecks` (submit a batch), `BatchEligibilityPolling`
- `GetBatch`, `GetBatchItems`
- `GetEligibilityCheckPdf`

## enrollment — provider transaction enrollment (~15 ops)

Manage which providers are enrolled with which payers for which transactions,
plus supporting documents and tasks.

- Enrollments: `ListEnrollments`, `CreateEnrollment`, `GetEnrollment`,
  `UpdateEnrollment`, `DeleteEnrollment`, `ExportEnrollmentsCsv`
- Documents: `CreateEnrollmentDocumentUpload`, `CreateEnrollmentDocumentDownload`,
  `DeleteEnrollmentDocument`
- Providers: `ListProviders`, `CreateProvider`, `GetProvider`, `UpdateProvider`,
  `DeleteProvider`
- Tasks: `UpdateTaskPost`

## claims — claim attachments (~2 ops)

Attach supporting files to claims.

- `CreateClaimAttachmentFile`, `SubmitClaimAttachmentRawX12`

## core — the raw EDI/X12 engine (~30 ops)

Lower-level building blocks: executions (pipeline runs), transactions, trading
partnerships, fragments, and EDI generation. Reach here when working with the
EDI plumbing rather than a clinical transaction.

- Executions: `ListExecutions`, `GetExecution`, `RetryExecutions`,
  `ListExecutionFaults`, `ListExecutionTransactions`, and input/output/metadata
  document getters (`GetExecutionInputDocument`, `...OutputDocument`, `...Url` variants)
- Transactions: `ListTransactions`, `GetTransaction`, and input/output/attachment/
  fragment document getters (plus `...Url` presigned-URL variants)
- Partnerships & generation: `CreatePartnershipOutboundTransaction`,
  `CreateTransactionGroup`, `GenerateTransactionGroupEdi`, `GenerateEdi`,
  `CreateOutboundFragment`
- Polling: `ListPollingExecutions`, `ListPollingTransactions`
- Events: `RetryEvent`

## event-destinations — event log (~2 ops)

Inspect events Stedi emitted to your destinations.

- `ListEvents`, `GetEvent`

---

## Tips for choosing

- A clinical transaction (eligibility, claim status, claim submission, remits)? →
  **healthcare**.
- "Which payer / payer ID / does payer X support Y?" → **payers** (or the same
  ops in healthcare).
- Bulk eligibility? → **manager**.
- "Set up / list provider enrollments"? → **enrollment**.
- "Look at the X12", "why did this transaction fail", "retry the execution"? →
  **core**.
- "What events fired?" → **event-destinations**.

The `...Url` operations in `core` return a presigned download URL instead of the
document bytes — handy for large payloads.
