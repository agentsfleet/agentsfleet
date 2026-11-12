"use client";

import { createContext, useContext } from "react";

/** An explicit navigation choice, separate from a confirmed publication receipt. */
export type AuthoringIntent = {prompt?: string; publicationTarget?: string};
export const AuthoringIntentContext = createContext<AuthoringIntent>({});
export const useAuthoringIntent = () => useContext(AuthoringIntentContext);
