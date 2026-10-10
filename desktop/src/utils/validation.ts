import { z } from "zod";
import { parseServerAddress } from "../types";

/** Every field-shape rule the app enforces client-side, in one place —
 * schemas are the single source of truth for a rule, reused by whichever
 * form(s) need it instead of each one re-deriving its own trim/range check.
 * Async, side-effecting checks (e.g. `ExecutablePicker`'s `pathExists` probe)
 * deliberately stay out of this file — these are pure shape validation. */

export const serverAddressSchema = z
  .string()
  .trim()
  .min(1, "Indica la dirección del servidor (host:puerto o wss://host).")
  .refine(
    (address) => parseServerAddress(address) !== null,
    "Usa host:puerto, o una URL ws:// o wss:// (por ejemplo wss://warlay.codesoftlabs.com).",
  );

export const roomNameSchema = z.string().trim().min(1, "Ponle un nombre a tu sala.");

export const gamePortSchema = z
  .number({ invalid_type_error: "El puerto debe ser un número." })
  .int("El puerto debe ser un número entero.")
  .min(1, "El puerto debe estar entre 1 y 65535.")
  .max(65535, "El puerto debe estar entre 1 y 65535.");

/** Mirrors the hub's own `MAX_CHAT_MESSAGE_LEN` (`crates/matchmaking/src/hub.rs`). */
export const chatMessageSchema = z
  .string()
  .trim()
  .min(1, "Escribe algo antes de enviar.")
  .max(500, "El mensaje no puede superar los 500 caracteres.");

export const displayNameSchema = z.string().trim().min(1, "Ponle un nombre de guerra.");

export const gameVersionSchema = z
  .number({ invalid_type_error: "Debe ser un número." })
  .int("Debe ser un número entero.")
  .min(1, "La versión debe estar entre 1 y 255.")
  .max(255, "La versión debe estar entre 1 y 255.");

export const slotsTotalSchema = z
  .number({ invalid_type_error: "Debe ser un número." })
  .int("Debe ser un número entero.")
  .min(2, "Los slots deben estar entre 2 y 24.")
  .max(24, "Los slots deben estar entre 2 y 24.");

export const chatMessageLimitSchema = z
  .number({ invalid_type_error: "Debe ser un número." })
  .int("Debe ser un número entero.")
  .min(20, "El límite debe estar entre 20 y 2000.")
  .max(2000, "El límite debe estar entre 20 y 2000.");
