/**
 * System store — T038.
 * Holds HardwareTier loaded on app mount.
 */
import { createSignal } from "solid-js";
import * as api from "../api";

export const [hardwareTier, setHardwareTier] = createSignal<api.HardwareTier | null>(null);

export async function loadHardwareTier() {
  try {
    const tier = await api.getHardwareTier();
    setHardwareTier(tier);
    return tier;
  } catch (e) {
    console.error("Failed to load hardware tier:", e);
    return null;
  }
}
