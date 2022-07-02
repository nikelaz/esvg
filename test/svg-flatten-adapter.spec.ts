import SVG from '../src/svg';
import SVGFlattenAdapter from '../src/svg-flatten-adapter';
import removeWhitespace from './remove-whitespace';

describe('flatten()', () => {
  test('converts all shape types to path', () => {
    const inputSvg = new SVG(`
      <svg width="200" height="250">
        <rect x="60" y="10" rx="10" ry="10" width="30" height="30" />
        <circle cx="25" cy="75" r="20" />
        <ellipse cx="75" cy="75" rx="20" ry="5" />
        <line x1="10" x2="50" y1="110" y2="150" />
        <polyline points="60 110 65 120 70 115 75 130 80 125 85 140 90 135 95 150 100 145" />
        <polygon points="50 160 55 180 70 180 60 190 65 205 50 195 35 205 40 190 30 180 45 180" />
        <path d="M20,230 Q40,205 50,230 T90,230" />
      </svg>
    `);

    const expectedSvg = new SVG(`
      <svg width="200" height="250">
        <path rx="10" ry="10" d="M60,10 90,10 90,40 60,40z"/>
        <path d="M5,75a20,20 0 1,0 40,0a20,20 0 1,0 -40,0"/>
        <path d="M55,75a20,5 0 1,0 40,0a20,5 0 1,0 -40,0"/>
        <path d="M10,110 50,150"/>
        <path d="M60,110 65,120 70,115 75,130 80,125 85,140 90,135 95,150 100,145"/>
        <path d="M50,160 55,180 70,180 60,190 65,205 50,195 35,205 40,190 30,180 45,180z"/>
        <path d="M20,230 Q40,205 50,230 T90,230"/>
      </svg>
    `);

    const result = SVGFlattenAdapter.flatten(inputSvg);

    expect(removeWhitespace(result.toString())).toEqual(removeWhitespace(expectedSvg.toString()));
  });
});

export default SVGFlattenAdapter;
