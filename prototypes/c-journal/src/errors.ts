export class FeedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "FeedError";
  }
}

export class CorrectionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CorrectionError";
  }
}

export class PostingError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PostingError";
  }
}
