import type { IntRange } from 'type-fest'
import { booksData, type BooksData } from './data/books.js'
import { excelColumnName } from './utils.js'

export type UsjRoot = {
  version: string
  content: UsjContent[]
}

export type UsjContent =
  | ({
      type: 'USJ'
    } & UsjRoot)
  | {
      type: 'para'
      marker:
        | 'ide'
        | 'sts'
        | 'rem'
        | 'h'
        | `toc${IntRange<1, 4>}`
        | `toca${IntRange<1, 4>}`
        | `imt${IntRange<1, 5>}`
        | `is${IntRange<1, 3>}`
        | 'ip'
        | 'ipi'
        | 'im'
        | 'imi'
        | 'ipq'
        | 'imq'
        | 'ipr'
        | 'ipc'
        | `iq${IntRange<1, 4>}`
        | `ili${IntRange<1, 3>}`
        | 'ib'
        | 'iot'
        | `io${IntRange<1, 5>}`
        | 'iex'
        | 'imte'
        | 'ie'
        | `mt${IntRange<1, 5>}`
        | `mte${IntRange<1, 3>}`
        | 'cl'
        | 'cd'
        | `ms${IntRange<1, 4>}`
        | 'mr'
        | `s${IntRange<1, 5>}`
        | 'sr'
        | 'r'
        | 'd'
        | 'sp'
        | `sd${IntRange<1, 5>}`
        | 'p'
        | 'm'
        | 'po'
        | 'cls'
        | 'pr'
        | 'pc'
        | 'pm'
        | 'pmo'
        | 'pmc'
        | 'pmr'
        | `pi${IntRange<1, 4>}`
        | 'mi'
        | 'lit'
        | 'nb'
        | 'b'
        | `ph${IntRange<1, 4>}`
        | `q${IntRange<1, 5>}`
        | 'qr'
        | 'qc'
        | 'qa'
        | `qm${IntRange<1, 4>}`
        | 'qd'
        | 'lh'
        | `li${IntRange<1, 5>}`
        | 'lf'
        | `lim${IntRange<1, 5>}`
        | 'tr'
      content?: ParaContent[]
    }
  | ({
      type: 'char'
      marker:
        | 'add'
        | 'bk'
        | 'dc'
        | 'em'
        | 'jmp'
        | 'k'
        | 'nd'
        | 'ord'
        | 'pn'
        | 'png'
        | 'qt'
        | 'rb'
        | 'rq'
        | 'ref'
        | 'sig'
        | 'sls'
        | 'tl'
        | 'w'
        | 'wa'
        | 'wg'
        | 'wh'
        | 'wj'
        | 'addpn'
        | 'pro'
        | 'bd'
        | 'it'
        | 'bdit'
        | 'no'
        | 'sc'
        | 'sup'
        | 'pb'
        | 'ior'
        | 'iqt'
        | 'qac'
        | 'qs'
        | 'litl'
        | 'lik'
        | 'liv'
        | 'fr'
        | 'fq'
        | 'fqa'
        | 'fk'
        | 'ft'
        | 'fl'
        | 'fw'
        | 'fp'
        | 'fv'
        | 'fdc'
        | 'fm'
        | 'xo'
        | 'xop'
        | 'xk'
        | 'xq'
        | 'xt'
        | 'xta'
        | 'xot'
        | 'xnt'
        | 'xdc'
      content: ParaContent[]
    } & AttributesMap)
  | {
      type: 'book'
      marker: 'id'
      content: [string] | []
      code: Book
    }
  | {
      type: 'chapter'
      marker: 'c'
      number: string
      altnumber?: string
      pubnumber?: string
      sid: string
    }
  | {
      type: 'verse'
      marker: 'v'
      number: string
      altnumber?: string
      pubnumber?: string
      sid: string
    }
  | ({
      type: 'ms'
      marker: `qt${IntRange<1, 6>}${MilestoneSide}` | `ts${MilestoneSide}`
      content?: ParaContent[]
    } & AttributesMap)
  | {
      type: 'note'
      content: ParaContent[]
      marker: 'f' | 'fe' | 'ef' | 'x' | 'ex'
      caller: '+' | '-' | string
      category?: string
    }
  | {
      type: 'table'
      content: UsjContent[]
    }
  | {
      type: 'table:row'
      marker: 'tr'
      content: UsjContent[]
    }
  | {
      type: 'table:cell'
      marker:
        | `th${CellRange}`
        | `thr${CellRange}`
        | `thc${CellRange}`
        | `tc${CellRange}`
        | `tcr${CellRange}`
        | `tcc${CellRange}`
      content: ParaContent[]
      align: 'start' | 'center' | 'end'
    }
  | {
      type: 'sidebar'
      marker: 'esb'
      content: UsjContent[]
      category?: string
    }
  | ({
      type: 'figure'
      marker: 'fig'
      content: [string] | []
    } & AttributesMap)
  | ({
      type: 'ref'
      content: [string] | []
    } & AttributesMap)

export type ParaContent = UsjContent | string

export type Book = {
  [K in keyof BooksData['books']]: BooksData['books'][K] extends { usfm_id: infer T }
    ? T
    : BooksData['books'][K]
}[keyof BooksData['books']]

export type VerseRange = `${number}-${number}`

export type AttributesMap = { [attribute: string]: string }

export type MilestoneSide = '-s' | '-e'

export type CellRange = number | `${number}-${number}`

export type ContentMarker = (UsjContent & { type: 'para' | 'char' })['marker']

export function walkUsj(elements: ParaContent[], handler: (element: ParaContent) => boolean) {
  for (const element of elements) {
    if (!handler(element)) continue
    if (typeof element === 'string') continue

    switch (element.type) {
      case 'USJ':
      case 'para':
      case 'char':
      case 'ms':
      case 'note':
      case 'table':
      case 'table:row':
      case 'table:cell':
      case 'sidebar':
        if (element.content) {
          walkUsj(element.content, handler)
        }
    }
  }
}

export function normalizeAndCountNotes(
  elements: ParaContent[],
  startId: number = 0,
  noteCount: number = 0,
) {
  walkUsj(elements, (element) => {
    if (typeof element !== 'string' && element.type === 'note') {
      noteCount++
      if (element.caller === '+') {
        element.caller = excelColumnName(++startId)
      }
    }
    return true
  })
  return [startId, noteCount] as const
}

export function isTitlePara(content: UsjContent) {
  return content.type === 'para' && isTitleMarker(content.marker)
}

export function isTitleMarker(marker: ContentMarker) {
  return /^(mt[1-4]|mte[1-2]|ms[1-3]|mr|s[1-4]|sr|r|d|sp|sd[1-4])$/.test(marker)
}

export const bookVerseCounts: Record<Book, number[]> = Object.fromEntries(
  Object.values(booksData.books).map((b) =>
    typeof b !== 'string' ? [b.usfm_id, b.verse_counts] : [b, []],
  ),
)

export const MACHINE_REFERENCE_REGEX = /^[A-Z1-4]{3}(-[A-Z1-4]{3})? ?[a-z0-9\-:]*$/
