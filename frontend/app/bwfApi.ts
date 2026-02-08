import type { Book, UsjContent, UsjRoot, VerseRange } from './usj'

/**
 * `/v1`
 */
export type ApiV1 = {
  /**
   * `/bibles`
   */
  bibles: BiblesResponse
  /**
   * `/short`
   */
  short: {
    /**
     * `/create?bible={bible}&ref={ref}`
     */
    create: ShortCreateResponse
    /**
     * `/resolve?type={type}&value={value}`
     */
    resolve: ShortResolveResponse
  }
  /**
   * `/bible/{bible}`
   */
  bible: {
    /**
     * `/book/{book}`
     */
    book: BibleBookResponse
    /**
     * `/search?term={term}&start={start}&count={count}`
     */
    search: BibleSearchResponse
    /**
     * `/index
     */
    index: BibleIndexResponse
  }
}

export type BiblesResponse = {
  default_bible: string
  bibles: Record<string, BibleInfo>
}

export type BibleInfo = {
  display_name: string | null
}

export type ShortCreateResponse = {
  type: 'id' | 'encoded'
  value: string
}

export type ShortResolveResponse = BibleReference[]

export type BibleBookResponse = UsjRoot

export type BibleSearchResponse = {
  response_type: 'search_results' | 'scripture_passages'
  search_term: string
  total_results: number
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

export type HighlightsMap = Record<string, GenericRange<number>[]>

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

export type BibleIndexResponse = {
  words: Record<string, number>
}
