/// <reference types="@raycast/api">

/* 🚧 🚧 🚧
 * This file is auto-generated from the extension's manifest.
 * Do not modify manually. Instead, update the `package.json` file.
 * 🚧 🚧 🚧 */

/* eslint-disable @typescript-eslint/ban-types */

type ExtensionPreferences = {}

/** Preferences accessible in all the extension's commands */
declare type Preferences = ExtensionPreferences

declare namespace Preferences {
  /** Preferences accessible in the `show-file-info` command */
  export type ShowFileInfo = ExtensionPreferences & {}
  /** Preferences accessible in the `clear-file-metadata` command */
  export type ClearFileMetadata = ExtensionPreferences & {}
}

declare namespace Arguments {
  /** Arguments passed to the `show-file-info` command */
  export type ShowFileInfo = {}
  /** Arguments passed to the `clear-file-metadata` command */
  export type ClearFileMetadata = {}
}

