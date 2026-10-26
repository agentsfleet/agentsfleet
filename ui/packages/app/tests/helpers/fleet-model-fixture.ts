import { z } from "zod";
import type { FleetModelOwner } from "@/components/domain/chat/useFleetModel";
import { DraftClientFixture } from "./draft-client-fixture";
import { CANDIDATE_ID, DRAFT, DRAFT_ID, FLEET_DETAIL, FLEET_ID, MODEL, MODEL_REF, REVIEW, WORKSPACE, response } from "./draft-fixtures";

export const RUNTIME_MODEL = {...MODEL, ref: "model:runtime", label: "Runtime model", eligibility: {authoring: false, runtime: true}};
export const AUTHORING_MODEL = {...MODEL, ref: "model:helper", label: "Setup helper", eligibility: {authoring: true, runtime: false}};
export const MODEL_OWNER: FleetModelOwner = {account: "owner-account", workspace: WORKSPACE, subject: "creator", fleet: FLEET_ID, revision: "1", model: MODEL_REF};
const NativeModelSchema = z.object({runtime_model_ref: z.string()});
const CreateModelDraftSchema = z.object({operation_id: z.string()});
const runtime = z.record(z.string(), z.unknown()).parse(REVIEW.effective.definition.config["x-agentsfleet"]);

export class FleetModelFixture extends DraftClientFixture {
  readonly #created = new Set<string>();
  readonly applyKeys: string[] = [];
  readonly applied = new Set<string>();
  saveStatus = 201;
  loseApply = false;
  loseEdit = false;
  mismatch = false;
  revision = "1";
  cleanupFails = false;
  mount(): void {
    super.mount(); this.applyKeys.length = 0; this.applied.clear(); this.#created.clear();
    this.saveStatus = 201; this.loseApply = false; this.loseEdit = false; this.mismatch = false;
    this.revision = "1"; this.cleanupFails = false;
    this.#seed();
    this.override = async (call) => {
      if (call.path.endsWith("/models")) return response({items: [MODEL, RUNTIME_MODEL, AUTHORING_MODEL], next_cursor: null});
      if (call.path.endsWith(`/fleets/${FLEET_ID}`)) return response({...FLEET_DETAIL, status: "active", setup_revision: this.revision});
      if (call.path.endsWith(`/drafts/${DRAFT_ID}`) && call.method === "DELETE") return response(null, this.cleanupFails ? 500 : 204);
      if (call.path.endsWith("/drafts") && call.method === "POST") {
        const {operation_id} = CreateModelDraftSchema.parse(call.body);
        if (!this.#created.has(operation_id)) {this.#seed(); this.#created.add(operation_id);}
        return response(this.current, 201);
      }
      if (call.path.endsWith(`/drafts/${DRAFT_ID}`) && call.method === "PATCH") {
        const ref = NativeModelSchema.parse(call.body).runtime_model_ref;
        const definition = {...REVIEW.effective.definition, config: {...REVIEW.effective.definition.config, "x-agentsfleet": {...runtime, model: ref}}};
        this.current = {...this.current, candidate: {id: CANDIDATE_ID, revision: "2", definition, canonical_digest: "model-two", policy_semantics: REVIEW.policy_semantics},
          review: {...REVIEW, review_digest: "model-two", effective: {...REVIEW.effective, definition, runtime_model_ref: this.mismatch ? MODEL_REF : ref, target_setup_revision: this.revision}}};
        if (this.loseEdit) {this.loseEdit = false; throw new TypeError("Lost edit acknowledgment");}
        return response(this.current);
      }
      if (call.path.endsWith("/revisions")) {
        const key = call.headers.get("Idempotency-Key") ?? ""; this.applyKeys.push(key);
        if (this.saveStatus !== 201) return response({user_message: "Another editor saved a newer version."}, this.saveStatus);
        this.applied.add(key);
        if (this.loseApply) {this.loseApply = false; throw new TypeError("Lost apply acknowledgment");}
        return response({fleet_id: FLEET_ID, setup_revision: "2", setup_etag: '"setup-two"', etag: '"fleet-two"', schedules: []}, 201);
      }
      return undefined;
    };
  }
  #seed(): void {
    this.current = {...DRAFT, linked_fleet_id: FLEET_ID, review: {...REVIEW, effective: {...REVIEW.effective, target_setup_revision: this.revision}}};
  }
}
