// @ohos.file.picker stub for run-log-export.mts ONLY. Functional, not empty:
// save() drives the three answers the real picker can give (a URI, an empty
// dismissal, a rejection), pre-creates the destination file in the in-memory
// fs exactly like the real picker's default autoCreateEmptyFile=true, and
// records the options + context the module under test passed, so the suite can
// assert the document name, the UIAbilityContext plumbing and that the picker
// was NOT opened when there is nothing to export.

import { seedFile } from './log-export-fs-stub.mjs';

export const state = {
  mode: 'ok', // 'ok' | 'empty' | 'blank' | 'reject'
  rejectCode: 13900020,
  calls: 0,
  lastOptions: undefined,
  lastContextFilesDir: undefined,
  uri: 'file://docs/storage/Users/currentUser/Documents/netbird-logs-stub.txt'
};

export class DocumentSaveOptions {
  constructor() {
    this.newFileNames = undefined;
    this.defaultFilePathUri = undefined;
    this.fileSuffixChoices = undefined;
  }
}

export class DocumentViewPicker {
  constructor(context) {
    this.context = context;
  }

  async save(option) {
    state.calls += 1;
    state.lastOptions = option;
    state.lastContextFilesDir = this.context === undefined ? undefined : this.context.filesDir;
    if (state.mode === 'reject') {
      const error = new Error('stub-picker-rejected');
      error.code = state.rejectCode;
      throw error;
    }
    if (state.mode === 'empty') {
      return [];
    }
    if (state.mode === 'blank') {
      return [''];
    }
    // The real picker pre-creates the file it returns (default
    // autoCreateEmptyFile=true) and grants this app a temporary read/write
    // permission on that URI.
    seedFile(state.uri, '', 1700000001);
    return [state.uri];
  }
}

// The module shape the real '@ohos.file.picker' presents to
// `import picker from ...`.
export default { DocumentSaveOptions, DocumentViewPicker };
