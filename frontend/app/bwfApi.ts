import { bookVerseCounts, type Book, type UsjContent, type UsjRoot } from './usj'

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
     * `/info`
     */
    info: BibleInfoResponse
    /**
     * `/book/{book}`
     */
    book: BibleBookResponse
    /**
     * `/books`
     */
    books: BibleBooksResponse
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

export type ShortCreateResponse = {
  type: 'id' | 'encoded'
  value: string
}

export type ShortResolveResponse = BibleReference[]

export type BibleInfoResponse = BibleInfo

export type BibleBookResponse = UsjRoot

export type BibleBooksResponse = {
  books: Record<Book, BibleBookInfo>
  book_order: (Book | '')[]
}

export type BibleSearchResponse = {
  response_type: 'search_results' | 'scripture_passages'
  search_term: string
  total_results: number
  references: SearchResponseResult[]
}

export type BibleInfo = {
  display_name: string | null
  text_direction: TextDirection
  simple_book_names: Record<Book, string>
}

export type TextDirection = 'auto' | 'ltr' | 'rtl'

export type BibleBookInfo = {
  translated_book_info: TranslatedBookInfo
  chapters: BibleBookChapterInfo[]
}

export type BibleBookChapterInfo = {
  number: string
  alt_number?: string
  pub_number?: string
}

export type SearchResponseResult = ReferenceContent | InvalidReference

export type ReferenceContent = {
  reference: BibleReference
  translated_book_info: TranslatedBookInfo | null
  previous_chapter: ChapterReference | null
  next_chapter: ChapterReference | null
  content: UsjContent[] | null
  highlights?: HighlightsArray
}

export type BibleReference = {
  book: Book
  chapter: number
  verses: [number, number]
}

export type ChapterReference = {
  book: Book
  translated_book_info: TranslatedBookInfo | null
  chapter: number
}

export type HighlightsArray = Array<GenericRange<TextLocation>>

export type TextLocation = {
  usj_path: number[]
  char: number
}

export type GenericRange<T> = {
  start: T
  end: T
}

export type TranslatedBookInfo = {
  running_header: string | null
  long_book_name: string | null
  short_book_name: string | null
  book_abbreviation: string | null
}

export type InvalidReference = {
  invalid_reference: string
  details: ParseReferenceError
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

export function isFullChapter(ref: BibleReference) {
  return ref.verses[0] === 1 && ref.verses[1] === bookVerseCounts[ref.book][ref.chapter - 1]
}

export function getChapterLabel(content: ReferenceContent) {
  let label = getShortBookName(content.translated_book_info, content.reference.book)
  for (const element of content.content ?? []) {
    if (element.type === 'para' && element.marker === 'cl') {
      label = element.content?.[0]?.toString() ?? ''
    } else if (element.type === 'chapter') {
      label += ` ${element.pubnumber ?? element.number}`
    }
  }
  return label
}

export function getShortBookName(info: TranslatedBookInfo | null, fallback: Book) {
  return (
    info?.short_book_name ??
    info?.running_header ??
    info?.book_abbreviation ??
    info?.long_book_name ??
    fallback
  )
}
