import removeTransforms from '../src/remove-transforms';
import SVG from '../src/svg';
import removeWhitespace from './remove-whitespace';

describe('removeTransforms()', () => {
  test('removes translate transform from path', () => {
    const input = new SVG(`
      <svg height="210" width="400">
        <path d="M150 0 L75 200 L225 200 Z" transform="translate(20px, 20px)" />
      </svg>
    `);

    const expectedOutput = new SVG(`
      <svg height="210" width="400">
        <path d="M170 20L95 220 245 220Z"/>
      </svg>
    `);

    const result = removeTransforms(input);

    expect(removeWhitespace(expectedOutput.toString())).toEqual(removeWhitespace(result.toString()));
  });

  test('removes transform scale from path', () => {
    const input = new SVG(`
      <svg height="210" width="400">
        <path d="M150 0 L75 200 L225 200 Z" transform="scale(2)" />
      </svg>
    `);

    const expectedOutput = new SVG(`
      <svg height="210" width="400">
        <path d="M149.90862405286435 5.234924505375146L67.97441268593198 202.4956276565067 217.88303673879636 207.73055216188186Z"/>
      </svg>
    `);

    const result = removeTransforms(input);

    expect(removeWhitespace(expectedOutput.toString())).toEqual(removeWhitespace(result.toString()));
  });

  test('removes transform matrix from path', () => {
    const input = new SVG(`
      <svg height="210" width="400">
        <path d="M150 0 L75 200 L225 200 Z" transform="matrix(0.4, 0, 0.5, 1.2, 60, 10)" />
      </svg>
    `);

    const expectedOutput = new SVG(`
      <svg height="210" width="400">
        <path d="M120 10L190 250 250 250Z"/>
      </svg>
    `);

    const result = removeTransforms(input);

    expect(removeWhitespace(expectedOutput.toString())).toEqual(removeWhitespace(result.toString()));
  });
});
