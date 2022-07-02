import SVG from '../src/svg';
import { XmlDocument } from 'xmldoc';

describe('constructor()', () => {
  test('sets the correct properties', () => {
    const svgStr = '<svg></svg>';
    const svgDoc = new XmlDocument(svgStr);
    
    const svgInstance = new SVG(svgStr);
    
    expect(svgInstance.toString()).toEqual(svgStr);
    expect(svgInstance.getXmlDoc()).toEqual(svgDoc);
  });
});

describe('updateFromString()', () => {
  test('updates the xmlString and xmlDoc properties', () => {
    const svgStr = '<svg></svg>';
    const svgStr2 = '<svg><path /></svg>';
    const svgDoc2 = new XmlDocument(svgStr2);
    const svgInstance = new SVG(svgStr);
    
    svgInstance.updateFromString(svgStr2);

    expect(svgInstance.toString()).toEqual(svgStr2);
    expect(svgInstance.getXmlDoc()).toEqual(svgDoc2);
  });
});
