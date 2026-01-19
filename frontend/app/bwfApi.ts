import type { Book, UsjContent, UsjRoot, VerseRange } from './usj'

export type BookResponse = UsjRoot

export type SearchResponse = {
  response_type: 'search_results' | 'scripture_passages'
  search_term: string
  references: SearchResponseResult[]
}

export type SearchResponseResult =
  | {
      reference: BibleReference
      translated_book_name: string | null
      content: UsjContent[] | null
      highlights?: HighlightsMap
    }
  | {
      invalid_reference: string
      details: ParseReferenceError
    }

export type BibleReference = {
  book: Book
  chapter: number
  verses: VerseRange
}

export type HighlightsMap = { [text: string]: GenericRange<number>[] }

export type GenericRange<T> = {
  start: T
  end: T
}

export type ParseReferenceError =
  | {
      type: 'missing_chapter'
    }
  | {
      type: 'invalid_chapter'
      chapter: string
    }
  | {
      type: 'invalid_verse'
      verse: string
    }
  | {
      type: 'unknown_book'
      booK: string
      valid_otherwise: boolean
    }
  | {
      type: 'out_of_bounds_chapter'
      book: string
      chapter: number
    }
  | {
      type: 'out_of_bounds_verse'
      book: string
      chapter: number
      verse: number
    }
  | {
      type: 'out_of_order_verses'
      verses: [number, number]
    }

export type IndexResponse = {
  words: { [word: string]: number }
}
