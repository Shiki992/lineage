// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open PowerPoint deck, through Office.js. Bytes only, as in Word: Lineage
// reads the whole file and never edits slides or XML itself.
//
// PowerPoint differs from Word in three ways that shape the deck panel
// (decision 0018):
// - there are no tracked changes, and no way to replace the open file, so a
//   deck's closed tailored versions are updated as stored files from the
//   master's panel, and an open one by changing its slides: the engine says
//   which to delete, insert from the merged deck, and move (decision 0019);
// - an add-in can't open a file by its address: "Open" goes to PowerPoint for
//   the web for a file in OneDrive;
// - there is no save call: PowerPoint saves files in OneDrive itself, and the
//   person saves a file on this computer.

import type { DocumentHost, FileLocation, Ids, SlideHost, SlideOp } from "@lineage/shell";
import { LineageError } from "@lineage/shell";
import { CHECKPOINT, FAMILY, base64, describeOpenFile, idsFrom, readWholeFile } from "./common";

/** PowerPointApi 1.7 has the custom properties Lineage's ids live in. */
export const NEEDED = "1.7";

/** PowerPointApi 1.8 adds `Slide.moveTo`, the last kind of slide step. */
export const SLIDE_STEPS = "1.8";

export class PowerPointHost implements DocumentHost, SlideHost {
  /**
   * True when this PowerPoint can carry out every kind of slide step, so an
   * open tailored deck can bring changes in from its own panel.
   */
  canChangeSlides(): boolean {
    return Office.context.requirements.isSetSupported("PowerPointApi", SLIDE_STEPS);
  }

  /**
   * Carries out the engine's slide steps (decision 0019), one sync each so a
   * failure stops where it is. A changed slide comes as a delete followed by
   * an insert: deleted first, the inserted slide keeps its ids (the Phase 4
   * spike). Inserted slides take this deck's theme and layouts.
   * https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.presentation#powerpoint-powerpoint-presentation-insertslidesfrombase64-member(1)
   * https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.insertslideoptions
   * https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.slide#powerpoint-powerpoint-slide-moveto-member(1)
   */
  async applySlideOps(ops: readonly SlideOp[], source: Uint8Array): Promise<void> {
    if (ops.length === 0) {
      return;
    }
    const from = base64(source);
    await PowerPoint.run(async (context) => {
      const slides = context.presentation.slides;
      for (const op of ops) {
        switch (op.kind) {
          case "delete":
            slides.getItem(op.slide).delete();
            break;
          case "insert":
            context.presentation.insertSlidesFromBase64(from, {
              formatting: PowerPoint.InsertSlideFormatting.useDestinationTheme,
              sourceSlideIds: [op.slide],
              ...(op.after === null ? {} : { targetSlideId: op.after }),
            });
            break;
          case "move":
            slides.getItem(op.slide).moveTo(op.index);
            break;
        }
        await context.sync();
      }
    });
  }

  readBytes(): Promise<Uint8Array> {
    return readWholeFile("PowerPoint");
  }

  /**
   * The two ids, from the presentation's custom properties (PowerPointApi 1.7).
   * The engine reads the same properties from the bytes (docProps/custom.xml).
   * https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.custompropertycollection
   */
  async readIds(): Promise<Ids | null> {
    return PowerPoint.run(async (context) => {
      const props = context.presentation.properties.customProperties;
      const family = props.getItemOrNullObject(FAMILY);
      const checkpoint = props.getItemOrNullObject(CHECKPOINT);
      family.load("value");
      checkpoint.load("value");
      await context.sync();
      if (family.isNullObject || checkpoint.isNullObject) {
        return null;
      }
      return idsFrom(family.value, checkpoint.value);
    });
  }

  async writeIds(ids: Ids | null): Promise<void> {
    await PowerPoint.run(async (context) => {
      const props = context.presentation.properties.customProperties;
      if (ids) {
        props.add(FAMILY, ids.family);
        props.add(CHECKPOINT, ids.checkpoint);
      } else {
        props.getItemOrNullObject(FAMILY).delete();
        props.getItemOrNullObject(CHECKPOINT).delete();
      }
      await context.sync();
    });
  }

  /** Never called for decks: there's no way to replace the open presentation. */
  replaceContents(): Promise<void> {
    return Promise.reject(
      new LineageError(
        "Bring the changes in from this deck's panel, or update it from the master deck's.",
        "PowerPoint can't replace the open presentation",
      ),
    );
  }

  /** PowerPoint has no save call for add-ins; it saves files in OneDrive itself. */
  save(): Promise<void> {
    return Promise.resolve();
  }

  describe(): Promise<{ name: string; url: string | null }> {
    return Promise.resolve(describeOpenFile("This deck"));
  }

  /**
   * Opens a stored deck: in PowerPoint for the web for a file in OneDrive.
   * PowerPoint add-ins can't open a file on this computer.
   * https://learn.microsoft.com/javascript/api/office/office.ui#office-office-ui-openbrowserwindow-member(1)
   */
  openFile(location: FileLocation): Promise<void> {
    if (
      "webUrl" in location &&
      Office.context.requirements.isSetSupported("OpenBrowserWindowApi", "1.1")
    ) {
      Office.context.ui.openBrowserWindow(location.webUrl);
      return Promise.resolve();
    }
    return Promise.reject(
      new LineageError(
        "PowerPoint can't open that file from here. It's in Lineage's folder; open it from there.",
        "no way to open a local file",
      ),
    );
  }

  /**
   * Opens bytes as a new presentation in a new window: "open what they saw".
   * https://learn.microsoft.com/javascript/api/powerpoint#powerpoint-powerpoint-createpresentation-function(1)
   */
  async openCopy(bytes: Uint8Array): Promise<void> {
    await PowerPoint.createPresentation(base64(bytes));
  }
}
