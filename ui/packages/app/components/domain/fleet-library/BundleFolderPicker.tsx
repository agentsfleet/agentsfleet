"use client";

import type { ChangeEvent } from "react";
import { useId, useState } from "react";
import { Button } from "@agentsfleet/design-system";
import { FolderOpenIcon } from "lucide-react";

import { BUNDLE_READ, readBundleFolder, SKILL_FILE_NAME, TRIGGER_FILE_NAME } from "./bundle-files";

const CHOOSE_FOLDER_LABEL = "Choose bundle folder";
const CHOOSE_FOLDER_HINT =
  `Reads ${SKILL_FILE_NAME} and ${TRIGGER_FILE_NAME} from the folder into the boxes below, so you can read what you are about to send.`;
const READ_FAILED =
  "That folder could not be read — the files may have changed since you picked them. Try again.";
// A failed replacement does not clear the boxes — the bytes below stay the
// truth surface — so the notice must say the old bundle is what would be sent.
const KEPT_PREVIOUS = " The previously loaded bundle is still filled in below.";

type Props = {
  /** Receives both bodies once a chosen folder resolves to a complete bundle. */
  onLoaded: (skillMarkdown: string, triggerMarkdown: string) => void;
};

/**
 * The folder affordance for the fleet-library dialog's local-bundle source.
 *
 * It owns its own notice because the tab around it unmounts when the dialog
 * closes or the source switches — that unmount is what clears the message, so
 * no caller has to remember to.
 */
export function BundleFolderPicker({ onLoaded }: Props) {
  const inputId = useId();
  const [notice, setNotice] = useState<string | null>(null);
  const [hasLoaded, setHasLoaded] = useState(false);

  async function handleChange(event: ChangeEvent<HTMLInputElement>) {
    const input = event.target;
    try {
      const bundle = await readBundleFolder(input.files);
      if (bundle.status === BUNDLE_READ.empty) return;
      if (bundle.status === BUNDLE_READ.refused) {
        setNotice(hasLoaded ? bundle.reason + KEPT_PREVIOUS : bundle.reason);
        return;
      }
      onLoaded(bundle.skillMarkdown, bundle.triggerMarkdown);
      setHasLoaded(true);
      setNotice(`Loaded ${SKILL_FILE_NAME} and ${TRIGGER_FILE_NAME}.`);
    } catch {
      // A File handle goes stale when the file is edited, moved or deleted
      // between the pick and the read, and then text() rejects. Say so —
      // an unhandled rejection here would leave whatever the last successful
      // pick loaded sitting under a "Loaded" line that no longer describes it.
      setNotice(hasLoaded ? READ_FAILED + KEPT_PREVIOUS : READ_FAILED);
    } finally {
      // Clearing the input lets the same folder be picked again after a
      // correction on disk — an unchanged value fires no change event. In
      // `finally` so a failed read cannot wedge the control shut. It also
      // drops the FileList, so it runs only once the bodies have been read.
      input.value = "";
    }
  }

  return (
    <div className="space-y-2">
      {/* A label rather than a click handler: pointing it at the input is what
          opens the picker, so the button needs no ref and no JavaScript, and
          the label text becomes the input's accessible name. */}
      <Button asChild variant="secondary">
        <label htmlFor={inputId}>
          <FolderOpenIcon size={14} />
          {CHOOSE_FOLDER_LABEL}
        </label>
      </Button>
      {/* Visually hidden: the design system wraps no file input, and the native
          control cannot be restyled into one. `webkitdirectory` is the attribute
          that makes a picker offer directories — this surface takes a folder,
          never loose files. */}
      <input
        id={inputId}
        type="file"
        webkitdirectory=""
        className="sr-only"
        onChange={(e) => { void handleChange(e); }}
      />
      <p className="text-body-sm text-muted-foreground">{notice ?? CHOOSE_FOLDER_HINT}</p>
    </div>
  );
}
